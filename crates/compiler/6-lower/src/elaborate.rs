//! Lowering puts a `Drop` wherever a value could be done with: where a variable's scope ends,
//! and after each statement for its temporaries. [`FnBuilder::elaborate_drops`] then works out
//! along every path what's still there: a moved value isn't dropped, one moved on some paths
//! only gets a flag saying whether it's still there, and of a struct with some fields moved
//! out only the rest are dropped. It also drops what an assignment replaces.

use std::collections::{BTreeMap, HashMap};

use som_base::Span;
use som_mir::*;

use super::FnBuilder;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Init {
    No,
    Yes,
    /// On some paths here and not on others.
    Maybe,
}

/// For each place that's a variable or a field of one, whether it's there. A place without
/// an entry is as the nearest place around it with one.
#[derive(Clone, PartialEq, Debug)]
struct State(BTreeMap<Place, Init>);

impl State {
    fn get(&self, place: &Place) -> Init {
        (0..=place.projection.len())
            .rev()
            .find_map(|len| {
                self.0.get(&Place {
                    local: place.local,
                    projection: place.projection[..len].to_vec(),
                })
            })
            .copied()
            .unwrap_or(Init::No)
    }

    fn set(&mut self, place: &Place, init: Init) {
        self.0
            .retain(|p, _| !(p.local == place.local && within(p, place) && p != place));
        self.0.insert(place.clone(), init);
    }

    /// Whether some place inside `place` isn't as `place` is: a field moved out of it.
    fn partly(&self, place: &Place) -> bool {
        let whole = self.get(place);
        (self.0.iter()).any(|(p, &init)| p != place && within(p, place) && init != whole)
    }

    fn join(&mut self, other: &State) -> bool {
        let keys: Vec<Place> = self.0.keys().chain(other.0.keys()).cloned().collect();
        let mut joined = BTreeMap::new();
        for key in keys {
            let (a, b) = (self.get(&key), other.get(&key));
            joined.insert(key, if a == b { a } else { Init::Maybe });
        }
        let changed = joined != self.0;
        self.0 = joined;
        changed
    }

    fn moved_out(&mut self, rvalue: &Rvalue) {
        for moved in rvalue.moves() {
            if followed(moved) {
                self.set(moved, Init::No);
            }
        }
    }
}

/// Whether `inner` is `outer` or inside it.
fn within(inner: &Place, outer: &Place) -> bool {
    inner.local == outer.local && inner.projection.starts_with(&outer.projection)
}

/// Whether `place` is a variable or a field of one, whose being there is followed.
fn followed(place: &Place) -> bool {
    place.local != RETURN && (place.projection.iter()).all(|p| matches!(p, Projection::Field(_)))
}

fn transfer(state: &mut State, kind: &StatementKind) {
    match kind {
        StatementKind::Assign(dest, rvalue) => {
            state.moved_out(rvalue);
            if followed(dest) {
                state.set(dest, Init::Yes);
            }
        }
        StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
            state.set(&Place::local(*local), Init::No);
        }
        StatementKind::Drop { place, .. } => {
            if followed(place) {
                state.set(place, Init::No);
            }
        }
    }
}

/// The flag saying whether each place dropped on some paths only is still there.
type Flags = HashMap<Place, Local>;

impl FnBuilder<'_, '_> {
    /// Whether it's still there is worked out later.
    pub(super) fn drop_local(&mut self, local: Local, span: Span) {
        let ty = self.locals[local].ty;
        if let Some(glue) = self.l.drop_glue(ty) {
            self.push(
                StatementKind::Drop {
                    place: Place::local(local),
                    glue,
                    flag: None,
                },
                span,
            );
        }
    }

    /// Drops `local`, then it stops existing. `som::mem::forget` doesn't drop what it's
    /// given: that's what it's for.
    pub(super) fn end_local(&mut self, local: Local, span: Span) {
        let params = self.param_count();
        if !(self.is_forget() && (1..=params).contains(&local.id)) {
            self.drop_local(local, span.clone());
        }
        self.push(StatementKind::StorageDead(local), span);
    }

    fn is_forget(&self) -> bool {
        let Some(func) = self.func else {
            return false;
        };
        let def = self.l.names.func(func);
        &*self.l.ast.funcs[func].name.name == som_lang::FORGET
            && &*self.l.ast.modules[self.l.names.def(def).module].name == som_lang::MEM_MODULE
    }

    pub(super) fn elaborate_drops(&mut self, body: &mut Body) {
        self.drop_temps_at_returns(body);
        // Parameters are there on entry, and nothing else.
        let mut start = State(BTreeMap::new());
        for arg in body.args() {
            start.set(&Place::local(arg), Init::Yes);
        }
        let entry = dataflow::forward(
            body,
            start.clone(),
            |id, state| {
                for stmt in &body.blocks[id].stmts {
                    transfer(state, &stmt.kind);
                }
            },
            State::join,
        );
        let mut flags = Flags::new();
        let ids: Vec<BlockId> = body.blocks.iter_with_ids().map(|(id, _)| id).collect();
        for &id in &ids {
            match entry[id.id].clone() {
                Some(state) => self.elaborate_block(body, id, state, &mut flags),
                // Never reached: nothing to drop.
                None => (body.blocks[id].stmts)
                    .retain(|s| !matches!(s.kind, StatementKind::Drop { .. })),
            }
        }
        if !flags.is_empty() {
            self.keep_flags(body, &ids, &flags, &start);
        }
    }

    /// Temporaries still there when the function returns go then: those of a loop's
    /// condition, say, which no statement ends.
    fn drop_temps_at_returns(&mut self, body: &mut Body) {
        let temps: Vec<Local> = (body.locals.iter_with_ids())
            .filter(|(id, decl)| *id != RETURN && id.id > body.arg_count && decl.name.is_none())
            .map(|(id, _)| id)
            .collect();
        let returns: Vec<BlockId> = (body.blocks.iter_with_ids())
            .filter(|(_, b)| matches!(b.terminator.kind, TerminatorKind::Return))
            .map(|(id, _)| id)
            .collect();
        for block in returns {
            for &temp in &temps {
                let ty = body.locals[temp].ty;
                if let Some(glue) = self.l.drop_glue(ty) {
                    let span = body.blocks[block].terminator.span.clone();
                    let drop = drop_statement(Place::local(temp), glue, None, &span);
                    body.blocks[block].stmts.push(drop);
                }
            }
        }
    }

    /// Each drop as what's there says; and what an assignment replaces, dropped.
    fn elaborate_block(
        &mut self,
        body: &mut Body,
        id: BlockId,
        mut state: State,
        flags: &mut Flags,
    ) {
        let stmts = std::mem::take(&mut body.blocks[id].stmts);
        let mut out = Vec::new();
        for stmt in stmts {
            match stmt.kind {
                StatementKind::Drop { place, .. } => {
                    self.drops_of(
                        &mut body.locals,
                        &state,
                        &place,
                        flags,
                        &stmt.span,
                        &mut out,
                    );
                    if followed(&place) {
                        state.set(&place, Init::No);
                    }
                }
                StatementKind::Assign(dest, rvalue) => {
                    let assign = Statement {
                        kind: StatementKind::Assign(dest, rvalue),
                        ..stmt
                    };
                    self.assign_replacing(body, &mut state, assign, flags, &mut out);
                }
                kind => {
                    transfer(&mut state, &kind);
                    out.push(Statement { kind, ..stmt });
                }
            }
        }
        body.blocks[id].stmts = out;
    }

    /// An assignment, dropping what it replaces: `new = value; drop(dest); dest = new`.
    fn assign_replacing(
        &mut self,
        body: &mut Body,
        state: &mut State,
        stmt: Statement,
        flags: &mut Flags,
        out: &mut Vec<Statement>,
    ) {
        let StatementKind::Assign(dest, rvalue) = stmt.kind else {
            unreachable!("an assignment")
        };
        let ty = self.l.mir.place_ty(&body.locals, &dest);
        let replaced = self.l.drop_glue(ty).is_some()
            && !body.outs.contains(&dest.local)
            && dest.local != RETURN;
        let mut drops = Vec::new();
        if replaced {
            if followed(&dest) {
                // After the value's worked out: it may read what's replaced.
                let mut after_moves = state.clone();
                after_moves.moved_out(&rvalue);
                self.drops_of(
                    &mut body.locals,
                    &after_moves,
                    &dest,
                    flags,
                    &stmt.span,
                    &mut drops,
                );
            } else if let Some(glue) = self.l.drop_glue(ty) {
                drops.push(drop_statement(dest.clone(), glue, None, &stmt.span));
            }
        }
        let kind = StatementKind::Assign(dest.clone(), rvalue);
        transfer(state, &kind);
        if drops.is_empty() {
            out.push(Statement { kind, ..stmt });
            return;
        }
        let StatementKind::Assign(_, rvalue) = kind else {
            unreachable!("an assignment")
        };
        let new = body.locals.alloc(LocalDecl {
            ty,
            name: None,
            mutable: false,
            span: stmt.span.clone(),
        });
        out.push(Statement {
            kind: StatementKind::Assign(Place::local(new), rvalue),
            span: stmt.span.clone(),
            reads: stmt.reads,
        });
        out.extend(drops);
        out.push(Statement {
            kind: StatementKind::Assign(dest, Rvalue::Use(Operand::Move(Place::local(new)))),
            span: stmt.span,
            reads: Vec::new(),
        });
    }

    /// Sets each flag where what it follows is assigned, and clears it where that's moved out
    /// of or dropped. Parameters are there to begin with; everything else isn't.
    fn keep_flags(&self, body: &mut Body, ids: &[BlockId], flags: &Flags, start: &State) {
        let bool = self.l.mir.types.bool;
        let set = |flag: Local, value: bool, span: &Span| Statement {
            kind: StatementKind::Assign(
                Place::local(flag),
                Rvalue::Use(Operand::Const(Const {
                    kind: ConstKind::Bool(value),
                    ty: bool,
                })),
            ),
            span: span.clone(),
            reads: Vec::new(),
        };
        for &id in ids {
            let stmts = std::mem::take(&mut body.blocks[id].stmts);
            let mut out = Vec::new();
            for stmt in stmts {
                let updates: Vec<Statement> = flag_updates(&stmt.kind, flags)
                    .into_iter()
                    .map(|(flag, value)| set(flag, value, &stmt.span))
                    .collect();
                out.push(stmt);
                out.extend(updates);
            }
            body.blocks[id].stmts = out;
        }
        let span = body.blocks[ids[0]].terminator.span.clone();
        let mut initial: Vec<Statement> = (flags.iter())
            .map(|(place, &flag)| set(flag, start.get(place) == Init::Yes, &span))
            .collect();
        initial.append(&mut body.blocks[ids[0]].stmts);
        body.blocks[ids[0]].stmts = initial;
    }

    /// The drops of what's at `place`, as `state` says it is, onto `out`: none if it's gone,
    /// one if it's there, one with a flag if it might be, and if fields of it were moved out,
    /// the drops of the others.
    fn drops_of(
        &mut self,
        locals: &mut Arena<LocalDecl>,
        state: &State,
        place: &Place,
        flags: &mut Flags,
        span: &Span,
        out: &mut Vec<Statement>,
    ) {
        let ty = self.l.mir.place_ty(locals, place);
        let Some(glue) = self.l.drop_glue(ty) else {
            return;
        };
        if !followed(place) {
            out.push(drop_statement(place.clone(), glue, None, span));
            return;
        }
        if state.partly(place) {
            // Only a struct's fields can be moved out, and not one with a `drop` of its own.
            let Ty::Struct(def, args) = self.l.mir.types.get(ty).clone() else {
                return;
            };
            let strukt = self.l.mir.struct_def(def, &args);
            if strukt.variants.is_some() || self.l.mir.drop_types.contains(&ty) {
                return;
            }
            for i in 0..strukt.fields.len() {
                let field = place.clone().project(Projection::Field(i));
                self.drops_of(locals, state, &field, flags, span, out);
            }
            return;
        }
        match state.get(place) {
            Init::No => {}
            Init::Yes => out.push(drop_statement(place.clone(), glue, None, span)),
            Init::Maybe => {
                let bool = self.l.mir.types.bool;
                let flag = *flags.entry(place.clone()).or_insert_with(|| {
                    locals.alloc(LocalDecl {
                        ty: bool,
                        name: None,
                        mutable: true,
                        span: span.clone(),
                    })
                });
                out.push(drop_statement(place.clone(), glue, Some(flag), span));
            }
        }
    }
}

/// The flags a statement sets (`true`) or clears, in order.
fn flag_updates(kind: &StatementKind, flags: &Flags) -> Vec<(Local, bool)> {
    let mut updates = Vec::new();
    match kind {
        StatementKind::Assign(dest, rvalue) => {
            for moved in rvalue.moves() {
                for (place, &flag) in flags {
                    if within(place, moved) || within(moved, place) {
                        updates.push((flag, false));
                    }
                }
            }
            for (place, &flag) in flags {
                if followed(dest) && within(place, dest) {
                    updates.push((flag, true));
                }
            }
        }
        StatementKind::Drop { place, .. } => {
            for (followed_place, &flag) in flags {
                if within(followed_place, place) {
                    updates.push((flag, false));
                }
            }
        }
        StatementKind::StorageDead(local) | StatementKind::StorageLive(local) => {
            for (followed_place, &flag) in flags {
                if followed_place.local == *local {
                    updates.push((flag, false));
                }
            }
        }
    }
    updates
}

fn drop_statement(place: Place, glue: FuncId, flag: Option<Local>, span: &Span) -> Statement {
    Statement {
        kind: StatementKind::Drop { place, glue, flag },
        span: span.clone(),
        reads: Vec::new(),
    }
}
