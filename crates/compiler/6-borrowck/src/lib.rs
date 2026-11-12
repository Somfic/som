//! Borrow checking on MIR.
//!
//! A forward dataflow analysis over each function's blocks tracks, at every
//! point:
//! - which places may have been moved out of, and
//! - which locals hold which loans (the result of a `&` or `&mut`).
//!
//! Loans are lexical: a loan lives as long as some live local holds it. It's
//! created by a borrow, flows into anything assigned from its holder whose type
//! contains a reference (including struct fields and function results), and ends
//! when the last holder goes out of scope (`StorageDead`). Temporaries hand their
//! loans on when they are read. Branches merge by union, and loops are iterated
//! until nothing changes, so "may have been moved" and "may still be borrowed"
//! hold along every path.
//!
//! Using a place then conflicts with the loans alive at that point, and a local
//! going out of scope while something still holds a loan on it is a dangling
//! reference.

mod error;

pub use error::BorrowError;

use som_base::Span;
use som_mir::*;
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

pub fn check(mir: &Mir) -> Vec<BorrowError> {
    let mut errors = Vec::new();
    for func in &mir.funcs {
        if let Some(body) = &func.body {
            FnChecker::new(mir, body).run(&mut errors);
        }
    }
    errors
}

type LoanId = usize;

struct Loan {
    place: Place,
    mutable: bool,
    span: Span,
}

/// What's known at one program point.
#[derive(Clone, PartialEq)]
struct State {
    /// Places that may have been moved out of, with where.
    moved: BTreeMap<Place, Span>,
    /// For each local, the loans it may hold.
    holds: Vec<BTreeSet<LoanId>>,
}

impl State {
    /// Merges `other` into `self`, returning whether anything changed.
    fn join(&mut self, other: &State) -> bool {
        let before = self.clone();
        for (place, span) in &other.moved {
            self.moved
                .entry(place.clone())
                .or_insert_with(|| span.clone());
        }
        for (mine, theirs) in self.holds.iter_mut().zip(&other.holds) {
            mine.extend(theirs);
        }
        *self != before
    }

    fn active(&self) -> BTreeSet<LoanId> {
        self.holds.iter().flatten().copied().collect()
    }

    fn release(&mut self, local: Local) {
        self.holds[local.0 as usize].clear();
        self.moved.retain(|place, _| place.local != local);
    }
}

/// Whether two places refer to overlapping memory: the same local, and one path extends the other.
fn overlaps(a: &Place, b: &Place) -> bool {
    a.local == b.local
        && (a.projection.starts_with(&b.projection) || b.projection.starts_with(&a.projection))
}

struct FnChecker<'a> {
    mir: &'a Mir,
    body: &'a Body,
    loans: Vec<Loan>,
    /// `(block, statement)` of each borrow → its loan.
    loan_at: BTreeMap<(usize, usize), LoanId>,
    /// Whether to report errors. Off while iterating to a fixpoint, on for the final pass.
    report: bool,
    errors: Vec<BorrowError>,
    reported: HashSet<(String, usize)>,
}

impl<'a> FnChecker<'a> {
    fn new(mir: &'a Mir, body: &'a Body) -> Self {
        let mut loans = Vec::new();
        let mut loan_at = BTreeMap::new();
        for (b, block) in body.blocks.iter().enumerate() {
            for (s, stmt) in block.stmts.iter().enumerate() {
                if let StatementKind::Assign(_, Rvalue::Ref { mutable, place }) = &stmt.kind {
                    loan_at.insert((b, s), loans.len());
                    loans.push(Loan {
                        place: place.clone(),
                        mutable: *mutable,
                        span: stmt.span.clone(),
                    });
                }
            }
        }
        FnChecker {
            mir,
            body,
            loans,
            loan_at,
            report: false,
            errors: Vec::new(),
            reported: HashSet::new(),
        }
    }

    fn run(mut self, errors: &mut Vec<BorrowError>) {
        let blocks = self.body.blocks.len();
        let mut entry: Vec<Option<State>> = vec![None; blocks];
        entry[0] = Some(State {
            moved: BTreeMap::new(),
            holds: vec![BTreeSet::new(); self.body.locals.len()],
        });

        let mut queue = VecDeque::from([0]);
        while let Some(b) = queue.pop_front() {
            let mut state = entry[b].clone().expect("queued blocks have a state");
            self.block(b, &mut state);
            for next in self.body.blocks[b].terminator.kind.successors() {
                let next = next.0 as usize;
                let changed = match &mut entry[next] {
                    Some(existing) => existing.join(&state),
                    slot @ None => {
                        *slot = Some(state.clone());
                        true
                    }
                };
                if changed && !queue.contains(&next) {
                    queue.push_back(next);
                }
            }
        }

        // The states are final; walk every reachable block once more, reporting errors.
        self.report = true;
        for (b, state) in entry.into_iter().enumerate() {
            if let Some(mut state) = state {
                self.block(b, &mut state);
            }
        }
        errors.extend(self.errors);
    }

    fn block(&mut self, b: usize, state: &mut State) {
        let block = &self.body.blocks[b];
        for (s, stmt) in block.stmts.iter().enumerate() {
            match &stmt.kind {
                StatementKind::Assign(place, rvalue) => {
                    let flows = self.rvalue(state, (b, s), rvalue, &stmt.span);
                    self.write(state, place, &stmt.span);
                    let holds = &mut state.holds[place.local.0 as usize];
                    if place.projection.is_empty() {
                        holds.clear();
                    }
                    if self.has_ref(self.place_ty(place)) {
                        holds.extend(flows);
                    }
                }
                StatementKind::StorageLive(local) => state.release(*local),
                StatementKind::StorageDead(local) => self.storage_dead(state, *local, &stmt.span),
            }
        }
        if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind {
            self.operand(state, condition, &block.terminator.span);
        }
    }

    // --- Transfer functions ---

    /// Evaluates an rvalue, returning the loans that flow into its result.
    fn rvalue(
        &mut self,
        state: &mut State,
        at: (usize, usize),
        rvalue: &Rvalue,
        span: &Span,
    ) -> BTreeSet<LoanId> {
        match rvalue {
            Rvalue::Use(op) | Rvalue::Not(op) => self.operand(state, op, span),
            Rvalue::Binary { lhs, rhs, .. } => {
                let mut flows = self.operand(state, lhs, span);
                flows.extend(self.operand(state, rhs, span));
                flows
            }
            Rvalue::Struct { fields: ops, .. } | Rvalue::Call { args: ops, .. } => {
                let mut flows = BTreeSet::new();
                for op in ops {
                    flows.extend(self.operand(state, op, span));
                }
                flows
            }
            Rvalue::Ref { place, .. } => self.borrow(state, self.loan_at[&at], place, span),
        }
    }

    fn operand(&mut self, state: &mut State, op: &Operand, span: &Span) -> BTreeSet<LoanId> {
        match op {
            Operand::Const(_) => BTreeSet::new(),
            Operand::Copy(place) => self.read(state, place, span, false),
            Operand::Move(place) => self.read(state, place, span, true),
        }
    }

    fn read(
        &mut self,
        state: &mut State,
        place: &Place,
        span: &Span,
        moving: bool,
    ) -> BTreeSet<LoanId> {
        self.check_moved(state, place, span);
        for loan in state.active() {
            let loan = &self.loans[loan];
            if !overlaps(&loan.place, place) {
                continue;
            }
            if moving {
                self.error(BorrowError::MoveWhileBorrowed {
                    name: self.name(place),
                    span: span.clone(),
                    borrowed_at: loan.span.clone(),
                });
                break;
            } else if loan.mutable {
                self.error(BorrowError::UseWhileMutBorrowed {
                    name: self.name(place),
                    span: span.clone(),
                    borrowed_at: loan.span.clone(),
                });
                break;
            }
        }

        if moving {
            if place.projection.contains(&Projection::Deref) {
                self.error(BorrowError::MoveOutOfReference {
                    name: self.name(place),
                    span: span.clone(),
                });
            } else {
                state.moved.insert(place.clone(), span.clone());
            }
        }

        let holds = &mut state.holds[place.local.0 as usize];
        if self.is_temp(place.local) {
            std::mem::take(holds)
        } else {
            holds.clone()
        }
    }

    fn borrow(
        &mut self,
        state: &mut State,
        id: LoanId,
        place: &Place,
        span: &Span,
    ) -> BTreeSet<LoanId> {
        self.check_moved(state, place, span);
        let loan = &self.loans[id];
        for other in state.active() {
            let existing = &self.loans[other];
            // A loop can reach the same borrow again while its previous loan is still held.
            if other != id && overlaps(&existing.place, place) && (loan.mutable || existing.mutable)
            {
                self.error(BorrowError::ConflictingBorrow {
                    name: self.name(place),
                    span: span.clone(),
                    mutable: loan.mutable,
                    existing_at: existing.span.clone(),
                    existing_mutable: existing.mutable,
                });
                break;
            }
        }

        let mut flows = BTreeSet::from([id]);
        // Reborrowing through a reference (`&*r`) keeps the original loan alive too.
        if place.projection.contains(&Projection::Deref) {
            flows.extend(state.holds[place.local.0 as usize].iter().copied());
        }
        flows
    }

    fn write(&mut self, state: &mut State, place: &Place, span: &Span) {
        for loan in state.active() {
            let loan = &self.loans[loan];
            if overlaps(&loan.place, place) {
                self.error(BorrowError::AssignWhileBorrowed {
                    name: self.name(place),
                    span: span.clone(),
                    borrowed_at: loan.span.clone(),
                });
                break;
            }
        }
        // Assigning a place makes it, and everything inside it, usable again.
        state.moved.retain(|moved, _| {
            !(moved.local == place.local && moved.projection.starts_with(&place.projection))
        });
    }

    fn storage_dead(&mut self, state: &mut State, local: Local, span: &Span) {
        let holders_of = |state: &State, loan: LoanId| {
            state
                .holds
                .iter()
                .enumerate()
                .any(|(holder, loans)| holder != local.0 as usize && loans.contains(&loan))
        };
        let dangling: Vec<LoanId> = state
            .active()
            .into_iter()
            .filter(|&loan| self.loans[loan].place.local == local && holders_of(state, loan))
            .collect();
        for loan in dangling {
            let l = &self.loans[loan];
            self.error(BorrowError::DanglingReference {
                name: self.name(&l.place),
                borrowed_at: l.span.clone(),
                dies_at: span.clone(),
            });
            // Report each dangling reference once, not again at every later use.
            for holds in &mut state.holds {
                holds.remove(&loan);
            }
        }
        state.release(local);
    }

    fn check_moved(&mut self, state: &State, place: &Place, span: &Span) {
        if let Some((_, moved_at)) = state.moved.iter().find(|(moved, _)| overlaps(moved, place)) {
            self.error(BorrowError::UseAfterMove {
                name: self.name(place),
                span: span.clone(),
                moved_at: moved_at.clone(),
            });
        }
    }

    fn error(&mut self, error: BorrowError) {
        if !self.report {
            return;
        }
        let debug = format!("{error:?}");
        let kind = debug.split(' ').next().unwrap_or_default().to_string();
        let span = match &error {
            BorrowError::DanglingReference { borrowed_at, .. } => borrowed_at,
            BorrowError::UseAfterMove { span, .. }
            | BorrowError::MoveOutOfReference { span, .. }
            | BorrowError::MoveWhileBorrowed { span, .. }
            | BorrowError::ConflictingBorrow { span, .. }
            | BorrowError::UseWhileMutBorrowed { span, .. }
            | BorrowError::AssignWhileBorrowed { span, .. } => span,
        };
        if self.reported.insert((kind, span.start_offset)) {
            self.errors.push(error);
        }
    }

    // --- Places and types ---

    fn is_temp(&self, local: Local) -> bool {
        local != Local::RETURN && self.body.local(local).name.is_none()
    }

    fn place_ty(&self, place: &Place) -> TypeId {
        let mut ty = self.body.local(place.local).ty;
        for projection in &place.projection {
            ty = match (projection, self.mir.types.get(ty)) {
                (Projection::Deref, Ty::Ref { to, .. }) => *to,
                (Projection::Field(i), Ty::Struct(def)) => self.mir.struct_def(*def).fields[*i].1,
                _ => unreachable!("validated MIR has well-typed places"),
            };
        }
        ty
    }

    /// Whether values of `ty` can carry a borrow.
    fn has_ref(&self, ty: TypeId) -> bool {
        match self.mir.types.get(ty) {
            Ty::Ref { .. } => true,
            Ty::Struct(def) => self
                .mir
                .struct_def(*def)
                .fields
                .iter()
                .any(|&(_, f)| self.has_ref(f)),
            _ => false,
        }
    }

    /// A place as it would be written in som: `p.x`, `*r`.
    fn name(&self, place: &Place) -> String {
        let decl = self.body.local(place.local);
        let mut name = decl
            .name
            .clone()
            .unwrap_or_else(|| "temporary value".into());
        let mut ty = decl.ty;
        for projection in &place.projection {
            match (projection, self.mir.types.get(ty)) {
                (Projection::Deref, Ty::Ref { to, .. }) => {
                    name = format!("*{name}");
                    ty = *to;
                }
                (Projection::Field(i), Ty::Struct(def)) => {
                    let (field, field_ty) = &self.mir.struct_def(*def).fields[*i];
                    name = format!("{name}.{field}");
                    ty = *field_ty;
                }
                _ => unreachable!("validated MIR has well-typed places"),
            }
        }
        name
    }
}
