//! Loans are lexical: a loan lives as long as some live local holds it. It flows into
//! anything assigned from its holder whose type can hold a reference, and ends when the last
//! holder's storage dies. Temporaries hand their loans on when read. States merge by union,
//! so "may have been moved" and "may still be borrowed" hold along every path.
//!
//! A method call's `&mut` of its receiver is a two-phase borrow, as in Rust: until the call
//! uses it, it counts as shared, so `v.push(v.len())` can read `v`.

mod error;
mod outs;
mod place_name;
mod recursion;

pub use error::BorrowError;

use som_base::Span;
use som_mir::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub fn check(mir: &Mir) -> Vec<BorrowError> {
    let mut errors = Vec::new();
    for (id, func) in mir.funcs.iter_with_ids() {
        if let Some(body) = &func.body {
            FnChecker::new(mir, body).run(&mut errors);
            outs::check(body, &mut errors);
            recursion::check(id, func, body, &mut errors);
        }
    }
    // A generic function's copies are the same code: once is enough.
    let mut reported = HashSet::new();
    errors.retain(|e| match e {
        BorrowError::EndlessRecursion { span, .. } => {
            reported.insert((span.source.identifier().to_string(), span.start_offset))
        }
        _ => true,
    });
    errors
}

/// Iterates to a fixpoint without reporting, then walks each reachable block once more with
/// its final state, reporting.
fn fixpoint_then_report<S: Clone>(
    body: &Body,
    start: S,
    join: impl FnMut(&mut S, &S) -> bool,
    mut block: impl FnMut(usize, &mut S, bool),
) {
    let entry = dataflow::forward(body, start, |b, state| block(b.id, state, false), join);
    for (b, state) in entry.into_iter().enumerate() {
        if let Some(mut state) = state {
            block(b, &mut state, true);
        }
    }
}

type LoanId = usize;

struct Loan {
    place: Place,
    mutable: bool,
    span: Span,
    /// The temporary holding it, when it's a two-phase borrow.
    two_phase: Option<Local>,
}

#[derive(Clone, PartialEq)]
struct State {
    moved: BTreeMap<Place, Span>,
    /// By local.
    holds: Vec<BTreeSet<LoanId>>,
    /// Two-phase loans not used yet, which count as shared.
    reserved: BTreeSet<LoanId>,
}

impl State {
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
        self.reserved.extend(&other.reserved);
        *self != before
    }

    fn active(&self) -> BTreeSet<LoanId> {
        self.holds.iter().flatten().copied().collect()
    }

    fn release(&mut self, local: Local) {
        self.holds[local.id].clear();
        self.moved.retain(|place, _| place.local != local);
    }

    fn held_by_other_than(&self, loan: LoanId, local: Local) -> bool {
        (self.holds.iter().enumerate())
            .any(|(holder, loans)| holder != local.id && loans.contains(&loan))
    }

    /// So it's reported once, not again at every later use.
    fn forget(&mut self, loan: LoanId) {
        for holds in &mut self.holds {
            holds.remove(&loan);
        }
    }
}

/// Any two elements of an array might be the same one, so indexes all count as equal.
fn overlaps(a: &Place, b: &Place) -> bool {
    let same = |x: &Projection, y: &Projection| match (x, y) {
        (Projection::Index(_), Projection::Index(_)) => true,
        _ => x == y,
    };
    a.local == b.local
        && a.projection
            .iter()
            .zip(&b.projection)
            .all(|(x, y)| same(x, y))
}

struct FnChecker<'a> {
    mir: &'a Mir,
    body: &'a Body,
    loans: Vec<Loan>,
    /// By `(block, statement)` of the borrow.
    loan_at: BTreeMap<(usize, usize), LoanId>,
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
                if let StatementKind::Assign(holder, Rvalue::Ref { mutable, place }) = &stmt.kind {
                    let two_phase = (*mutable
                        && holder.projection.is_empty()
                        && body.two_phase.contains(&holder.local))
                    .then_some(holder.local);
                    loan_at.insert((b, s), loans.len());
                    loans.push(Loan {
                        place: place.clone(),
                        mutable: *mutable,
                        span: stmt.span.clone(),
                        two_phase,
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
        let start = State {
            moved: BTreeMap::new(),
            holds: vec![BTreeSet::new(); self.body.locals.len()],
            reserved: BTreeSet::new(),
        };
        fixpoint_then_report(self.body, start, State::join, |b, state, report| {
            self.report = report;
            self.block(b, state);
        });
        errors.extend(self.errors);
    }

    fn block(&mut self, b: usize, state: &mut State) {
        let block = &self.body.blocks[Id::new(b)];
        for (s, stmt) in block.stmts.iter().enumerate() {
            match &stmt.kind {
                StatementKind::Assign(place, rvalue) => {
                    let flows = self.rvalue(state, (b, s), rvalue, &stmt.reads, &stmt.span);
                    self.write(state, place, &stmt.span);
                    let holds = &mut state.holds[place.local.id];
                    if place.projection.is_empty() {
                        holds.clear();
                    }
                    if self.has_ref(self.place_ty(place)) {
                        holds.extend(flows);
                    }
                }
                StatementKind::StorageLive(local) => state.release(*local),
                StatementKind::StorageDead(local) => self.storage_dead(state, *local, &stmt.span),
                StatementKind::Drop { place, .. } => self.drop(state, place, &stmt.span),
            }
        }
        if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind {
            self.operand(state, condition, &block.terminator.span);
        }
    }

    /// The loans that flow into its result. Operands not among `reads` are at `span`.
    fn rvalue(
        &mut self,
        state: &mut State,
        at: (usize, usize),
        rvalue: &Rvalue,
        reads: &[(Place, Span)],
        span: &Span,
    ) -> BTreeSet<LoanId> {
        if let Rvalue::Ref { place, .. } = rvalue {
            let id = self.loan_at[&at];
            let flows = self.borrow(state, id, place, span);
            if self.loans[id].two_phase.is_some() {
                state.reserved.insert(id);
            }
            return flows;
        }
        let mut reads = reads.iter().peekable();
        let mut flows = BTreeSet::new();
        for op in rvalue.operands() {
            let read = match op {
                Operand::Copy(place) | Operand::Move(place) => {
                    reads.next_if(|(read, _)| read == place)
                }
                Operand::Const(_) => None,
            };
            let span = read.map_or(span, |(_, at)| at);
            flows.extend(self.operand(state, op, span));
        }
        flows
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
        self.activate(state, place.local);
        self.check_moved(state, place, span);
        for id in state.active() {
            let reserved = state.reserved.contains(&id);
            let loan = &self.loans[id];
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
            } else if loan.mutable && !reserved {
                self.error(BorrowError::UseWhileMutBorrowed {
                    name: self.name(place),
                    span: span.clone(),
                    borrowed_at: loan.span.clone(),
                });
                break;
            }
        }
        if moving {
            self.move_out(state, place, span);
        }
        let holds = &mut state.holds[place.local.id];
        if self.is_temp(place.local) {
            std::mem::take(holds)
        } else {
            holds.clone()
        }
    }

    fn move_out(&mut self, state: &mut State, place: &Place, span: &Span) {
        // What a type that implements `Drop` holds stays for its `drop`.
        let in_drop_type = (0..place.projection.len()).find_map(|len| {
            let outer = Place {
                local: place.local,
                projection: place.projection[..len].to_vec(),
            };
            let ty = self.place_ty(&outer);
            self.mir.drop_types.contains(&ty).then_some(ty)
        });
        if place.projection.contains(&Projection::Deref) {
            self.error(BorrowError::MoveOutOfReference {
                name: self.name(place),
                span: span.clone(),
            });
        } else if let Some(ty) = in_drop_type {
            self.error(BorrowError::MoveOutOfDrop {
                name: self.name(place),
                ty: self.mir.display_ty(ty),
                span: span.clone(),
            });
        } else {
            state.moved.insert(place.clone(), span.clone());
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
        // A two-phase borrow is shared until it's used, and checked again then.
        let mutable = loan.mutable && loan.two_phase.is_none();
        for other in state.active() {
            let existing = &self.loans[other];
            let existing_mutable = existing.mutable && !state.reserved.contains(&other);
            // A loop can reach the same borrow again while its previous loan is still held.
            if other != id && overlaps(&existing.place, place) && (mutable || existing_mutable) {
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
        // Reborrowing through a reference (`&*r`) keeps the original loan alive too. A
        // temporary is used once, as when it's read: its loans move to the reborrow.
        if place.projection.contains(&Projection::Deref) {
            let holds = &mut state.holds[place.local.id];
            if self.is_temp(place.local) {
                flows.extend(std::mem::take(holds));
            } else {
                flows.extend(holds.iter().copied());
            }
        }
        flows
    }

    /// From here `holder`'s two-phase loans are mutable, so they can't overlap any loan still
    /// alive, like one the call's arguments still hold.
    fn activate(&mut self, state: &mut State, holder: Local) {
        let activated: Vec<LoanId> = (state.reserved.iter().copied())
            .filter(|&id| self.loans[id].two_phase == Some(holder))
            .collect();
        for id in activated {
            state.reserved.remove(&id);
            let loan = &self.loans[id];
            let conflict = state.active().into_iter().find(|&other| {
                other != id
                    && overlaps(&self.loans[other].place, &loan.place)
                    && !(state.holds[holder.id].contains(&other))
            });
            if let Some(other) = conflict {
                let existing = &self.loans[other];
                self.error(BorrowError::ConflictingBorrow {
                    name: self.name(&loan.place),
                    span: loan.span.clone(),
                    mutable: true,
                    existing_at: existing.span.clone(),
                    existing_mutable: existing.mutable,
                });
            }
        }
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
        // A temporary is used once: `*at_mut(&mut xs, i) = v` is done with its loan.
        if self.is_temp(place.local) && place.projection.contains(&Projection::Deref) {
            state.holds[place.local.id].clear();
        }
    }

    /// A named variable's borrows are checked where its storage dies, right after; a
    /// temporary's, which has no such point, here.
    fn drop(&mut self, state: &mut State, place: &Place, span: &Span) {
        if !self.is_temp(place.local) {
            return;
        }
        let borrowed = state.active().into_iter().find(|&loan| {
            let l = &self.loans[loan].place;
            overlaps(l, place)
                && !l.projection.contains(&Projection::Deref)
                && state.held_by_other_than(loan, place.local)
        });
        if let Some(loan) = borrowed {
            let l = &self.loans[loan];
            self.error(BorrowError::TemporaryDropped {
                borrowed_at: l.span.clone(),
                dropped_at: span.clone(),
            });
            state.forget(loan);
        }
    }

    fn storage_dead(&mut self, state: &mut State, local: Local, span: &Span) {
        // A reborrow through a reference (`&(*r).x`) points at what `r` refers to, which
        // outlives `r`; the reborrow also holds `r`'s own loans, which keep that borrowed.
        let dangling: Vec<LoanId> = state
            .active()
            .into_iter()
            .filter(|&loan| {
                let place = &self.loans[loan].place;
                place.local == local
                    && !place.projection.contains(&Projection::Deref)
                    && state.held_by_other_than(loan, local)
            })
            .collect();
        for loan in dangling {
            let l = &self.loans[loan];
            self.error(BorrowError::DanglingReference {
                name: self.place_name(&l.place).if_named(),
                borrowed_at: l.span.clone(),
                dies_at: span.clone(),
            });
            state.forget(loan);
        }
        state.release(local);
    }

    fn check_moved(&mut self, state: &State, place: &Place, span: &Span) {
        if let Some((moved, moved_at)) =
            state.moved.iter().find(|(moved, _)| overlaps(moved, place))
        {
            // Name what was consumed (`p`), not the part read afterwards (`p.age`).
            self.error(BorrowError::UseAfterMove {
                name: self.name(moved),
                ty: self.mir.display_ty(self.place_ty(moved)),
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
            | BorrowError::AssignWhileBorrowed { span, .. }
            | BorrowError::OutNotAssigned { span, .. }
            | BorrowError::OutReadBeforeAssigned { span, .. }
            | BorrowError::EndlessRecursion { span, .. }
            | BorrowError::MoveOutOfDrop { span, .. } => span,
            BorrowError::TemporaryDropped { borrowed_at, .. } => borrowed_at,
        };
        if self.reported.insert((kind, span.start_offset)) {
            self.errors.push(error);
        }
    }

    fn is_temp(&self, local: Local) -> bool {
        local != RETURN && self.body.local(local).name.is_none()
    }

    fn place_ty(&self, place: &Place) -> TypeId {
        self.mir.place_ty(&self.body.locals, place)
    }

    fn has_ref(&self, ty: TypeId) -> bool {
        self.has_ref_in(ty, &mut HashSet::new())
    }

    /// Skips the types in `seen`: a struct can reach itself through a pointer,
    /// `struct Node { next: *Node }`, and what's found there was already found.
    fn has_ref_in(&self, ty: TypeId, seen: &mut HashSet<TypeId>) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        match self.mir.types.get(ty) {
            Ty::Ref { .. } => true,
            Ty::Struct(def, args) => (self.mir.struct_def(*def, args).fields.iter())
                .any(|&(_, f)| self.has_ref_in(f, seen)),
            // A function's parameters and result are only types: a function value holds no loan.
            Ty::Fn { .. } | Ty::FnItem(..) => false,
            t => t
                .children()
                .into_iter()
                .any(|child| self.has_ref_in(child, seen)),
        }
    }
}
