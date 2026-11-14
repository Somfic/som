//! Dropping values: what a type needs done when a value of it is done with, and where in a
//! function that happens.
//!
//! Lowering puts a `Drop` wherever a value could be done with: where a variable's scope
//! ends, and after each statement for its temporaries. [`FnBuilder::elaborate_drops`] then
//! works out, along every path, what's still there: a moved value isn't dropped, one moved
//! on some paths only gets a flag saying whether it's still there, and of a struct with some
//! fields moved out only the rest are dropped. It also drops what an assignment replaces.
//!
//! A type's drop glue is a function taking a reference to a value: it calls the type's
//! `Drop::drop`, if it implements it, then drops each field that needs it.

use std::collections::{BTreeMap, HashMap};

use som_ast::Id;
use som_mir::*;
use som_resolve::LangTrait;
use som_typeck::traits::find_impl;

use super::{FnBuilder, Lowerer};

impl Lowerer<'_> {
    /// The function dropping a value of type `ty`, or `None` if nothing needs doing.
    pub(super) fn drop_glue(&mut self, ty: TypeId) -> Option<FuncId> {
        if let Some(&glue) = self.drop_glues.get(&ty) {
            return glue;
        }
        // Until it's built: a type can't hold itself, other than behind a pointer.
        self.drop_glues.insert(ty, None);
        let glue = self.build_glue(ty);
        self.drop_glues.insert(ty, glue);
        glue
    }

    /// `ty`'s own `Drop::drop`, for its type arguments, if it implements it.
    fn drop_impl(&mut self, ty: TypeId) -> Option<FuncId> {
        let trait_ = self.names.lang(LangTrait::Drop)?;
        let (index, args) = find_impl(&self.typed.impls, &self.mir.types, trait_, ty, &[], &|t| t)?;
        let method = (self.typed.impls[index].methods.iter().copied())
            .find(|&m| &*self.ast.funcs[m].name.name == "drop")?;
        self.mir.drop_types.insert(ty);
        Some(self.instance(method, args))
    }

    fn build_glue(&mut self, ty: TypeId) -> Option<FuncId> {
        enum Shape {
            Struct {
                own: Option<FuncId>,
                /// Fields every value has, and for an enum, each variant's own.
                common: Vec<(usize, FuncId)>,
                variants: Vec<Vec<(usize, FuncId)>>,
            },
            Array { len: u64, element: FuncId },
            Optional(FuncId),
        }
        let shape = match self.mir.types.get(ty).clone() {
            Ty::Struct(def, args) => {
                self.make_structs_in(ty);
                let own_drop = self.drop_impl(ty);
                let own = own_drop;
                let strukt = self.mir.struct_def(def, &args);
                let fields: Vec<TypeId> = strukt.fields.iter().map(|(_, t)| *t).collect();
                let variants: Option<Vec<Vec<usize>>> = (strukt.variants.as_ref())
                    .map(|vs| vs.iter().map(|v| v.fields.clone()).collect());
                let mut glues = HashMap::new();
                for (i, &field) in fields.iter().enumerate() {
                    if let Some(glue) = self.drop_glue(field) {
                        glues.insert(i, glue);
                    }
                }
                let with_glue = |indexes: &mut dyn Iterator<Item = usize>| -> Vec<(usize, FuncId)> {
                    indexes.filter_map(|i| glues.get(&i).map(|&g| (i, g))).collect()
                };
                match variants {
                    // The tag, then fields in every variant, then each variant's own.
                    Some(variants) => {
                        let common: Vec<usize> = (1..fields.len())
                            .filter(|i| variants.iter().all(|v| v.contains(i)))
                            .collect();
                        let own: Vec<Vec<(usize, FuncId)>> = (variants.iter())
                            .map(|v| with_glue(&mut v.iter().copied().filter(|i| !common.contains(i))))
                            .collect();
                        let common = with_glue(&mut common.into_iter());
                        if own_drop.is_none() && common.is_empty() && own.iter().all(Vec::is_empty) {
                            return None;
                        }
                        Shape::Struct {
                            own: own_drop,
                            common,
                            variants: own,
                        }
                    }
                    None => {
                        let common = with_glue(&mut (0..fields.len()));
                        if own.is_none() && common.is_empty() {
                            return None;
                        }
                        Shape::Struct {
                            own,
                            common,
                            variants: Vec::new(),
                        }
                    }
                }
            }
            Ty::Array { element, len } => {
                let element = self.drop_glue(element)?;
                Shape::Array {
                    len: self.mir.types.len(len),
                    element,
                }
            }
            Ty::Optional(inner) => Shape::Optional(self.drop_glue(inner)?),
            _ => return None,
        };

        let span = self.span_of_type(ty);
        let unit = self.mir.types.unit;
        let reference = self.mir.types.intern(Ty::Ref {
            mutable: true,
            to: ty,
        });
        let mut g = Glue::new(span.clone(), unit, reference);
        let this = Place::local(Id::new(1)).project(Projection::Deref);
        match shape {
            Shape::Struct {
                own,
                common,
                variants,
            } => {
                if let Some(own) = own {
                    g.call_on(own, this.clone(), reference);
                }
                for (i, glue) in common {
                    let field = self.field_ty(ty, i);
                    g.drop_field(glue, this.clone().project(Projection::Field(i)), field, &mut self.mir.types);
                }
                // Only the variant the tag says holds anything.
                let u32 = self.mir.types.intern(Ty::Int(IntTy::U32));
                let bool = self.mir.types.bool;
                for (v, fields) in variants.into_iter().enumerate() {
                    if fields.is_empty() {
                        continue;
                    }
                    let is = g.local(bool);
                    let tag = Operand::Copy(this.clone().project(Projection::Field(0)));
                    let value = Operand::Const(Const {
                        kind: ConstKind::Int(v as i64),
                        ty: u32,
                    });
                    g.assign(
                        Place::local(is),
                        Rvalue::Binary {
                            op: BinOp::Equals,
                            lhs: tag,
                            rhs: value,
                        },
                    );
                    let (then, after) = (g.block(), g.block());
                    g.branch(Operand::Copy(Place::local(is)), then, after);
                    g.current = then;
                    for (i, glue) in fields {
                        let field = self.field_ty(ty, i);
                        g.drop_field(glue, this.clone().project(Projection::Field(i)), field, &mut self.mir.types);
                    }
                    g.goto(after);
                    g.current = after;
                }
            }
            // Each element, by index.
            Shape::Array { len, element } => {
                let usize = self.mir.types.usize;
                let bool = self.mir.types.bool;
                let element_ty = match *self.mir.types.get(ty) {
                    Ty::Array { element, .. } => element,
                    _ => unreachable!("an array's glue"),
                };
                let i = g.local(usize);
                let int = |n: i64| {
                    Operand::Const(Const {
                        kind: ConstKind::Int(n),
                        ty: usize,
                    })
                };
                g.assign(Place::local(i), Rvalue::Use(int(0)));
                let (header, body, exit) = (g.block(), g.block(), g.block());
                g.goto(header);
                g.current = header;
                let more = g.local(bool);
                g.assign(
                    Place::local(more),
                    Rvalue::Binary {
                        op: BinOp::LessThan,
                        lhs: Operand::Copy(Place::local(i)),
                        rhs: int(len as i64),
                    },
                );
                g.branch(Operand::Copy(Place::local(more)), body, exit);
                g.current = body;
                let item = this.clone().project(Projection::Index(i));
                g.drop_field(element, item, element_ty, &mut self.mir.types);
                g.assign(
                    Place::local(i),
                    Rvalue::Binary {
                        op: BinOp::Add,
                        lhs: Operand::Copy(Place::local(i)),
                        rhs: int(1),
                    },
                );
                g.goto(header);
                g.current = exit;
            }
            // What it holds, if it holds something.
            Shape::Optional(inner) => {
                let inner_ty = match *self.mir.types.get(ty) {
                    Ty::Optional(inner) => inner,
                    _ => unreachable!("an optional's glue"),
                };
                let (then, after) = (g.block(), g.block());
                g.branch(Operand::Copy(this.clone().project(Projection::Field(0))), then, after);
                g.current = then;
                g.drop_field(inner, this.clone().project(Projection::Field(1)), inner_ty, &mut self.mir.types);
                g.goto(after);
                g.current = after;
            }
        }
        g.assign(
            Place::local(RETURN),
            Rvalue::Use(Operand::Const(Const {
                kind: ConstKind::Unit,
                ty: unit,
            })),
        );
        g.terminate(TerminatorKind::Return);

        let name = format!("drop<{}>", self.mir.display_ty(ty));
        let id = self.mir.funcs.alloc(Function {
            name,
            params: vec![reference],
            ret: unit,
            span,
            body: Some(Body {
                locals: g.locals,
                arg_count: 1,
                outs: Vec::new(),
                blocks: g.blocks,
            }),
            link_name: None,
            library: None,
            is_entry: false,
            variadic: false,
        });
        Some(id)
    }

    /// The type of struct `ty`'s `index`th field.
    fn field_ty(&self, ty: TypeId, index: usize) -> TypeId {
        self.mir
            .project(ty, Projection::Field(index))
            .expect("a field of the struct")
    }

    /// Where a type is declared, for the glue's span: the struct, or the program's start.
    fn span_of_type(&self, ty: TypeId) -> som_base::Span {
        match *self.mir.types.get(ty) {
            Ty::Struct(def, _) => self.names.def(def).span.clone(),
            _ => (self.ast.funcs.iter().next())
                .map(|(id, _)| self.ast.funcs.span(id).clone())
                .expect("a program has a function"),
        }
    }
}

/// A drop glue function being built.
struct Glue {
    locals: Arena<LocalDecl>,
    blocks: Arena<Block>,
    current: BlockId,
    span: som_base::Span,
    unit: TypeId,
}

impl Glue {
    fn new(span: som_base::Span, unit: TypeId, reference: TypeId) -> Glue {
        let mut g = Glue {
            locals: Arena::new(),
            blocks: Arena::new(),
            current: Id::new(0),
            span,
            unit,
        };
        g.local(unit);
        g.local(reference);
        g.current = g.block();
        g
    }

    fn local(&mut self, ty: TypeId) -> Local {
        self.locals.alloc(LocalDecl {
            ty,
            name: None,
            mutable: true,
            span: self.span.clone(),
        })
    }

    fn block(&mut self) -> BlockId {
        self.blocks.alloc(Block {
            stmts: Vec::new(),
            terminator: Terminator {
                kind: TerminatorKind::Unreachable,
                span: self.span.clone(),
            },
        })
    }

    fn assign(&mut self, place: Place, rvalue: Rvalue) {
        let span = self.span.clone();
        self.blocks[self.current].stmts.push(Statement {
            kind: StatementKind::Assign(place, rvalue),
            span,
        });
    }

    fn terminate(&mut self, kind: TerminatorKind) {
        self.blocks[self.current].terminator = Terminator {
            kind,
            span: self.span.clone(),
        };
    }

    fn goto(&mut self, target: BlockId) {
        self.terminate(TerminatorKind::Goto(target));
    }

    fn branch(&mut self, condition: Operand, then: BlockId, otherwise: BlockId) {
        self.terminate(TerminatorKind::Branch {
            condition,
            then,
            otherwise,
        });
    }

    /// Calls `func` with `&mut place`, a `reference`.
    fn call_on(&mut self, func: FuncId, place: Place, reference: TypeId) {
        let lent = self.local(reference);
        self.assign(
            Place::local(lent),
            Rvalue::Ref {
                mutable: true,
                place,
            },
        );
        let done = self.local(self.unit);
        self.assign(
            Place::local(done),
            Rvalue::Call {
                func,
                args: vec![Operand::Move(Place::local(lent))],
            },
        );
    }

    /// Drops what's at `place`, of type `ty`, with its glue.
    fn drop_field(&mut self, glue: FuncId, place: Place, ty: TypeId, types: &mut TypeTable) {
        let reference = types.intern(Ty::Ref {
            mutable: true,
            to: ty,
        });
        self.call_on(glue, place, reference);
    }
}

/// Whether what's at a place is there.
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
        self.0.retain(|p, _| !(p.local == place.local && within(p, place) && p != place));
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
}

/// Whether `inner` is `outer` or inside it.
fn within(inner: &Place, outer: &Place) -> bool {
    inner.local == outer.local && inner.projection.starts_with(&outer.projection)
}

/// Whether `place` is a variable or a field of one, whose being there is followed.
fn followed(place: &Place) -> bool {
    place.local != RETURN
        && (place.projection.iter()).all(|p| matches!(p, Projection::Field(_)))
}

/// The places an rvalue moves out of.
fn moves(rvalue: &Rvalue) -> Vec<&Place> {
    let operands: Vec<&Operand> = match rvalue {
        Rvalue::Use(op)
        | Rvalue::Not(op)
        | Rvalue::Some(op)
        | Rvalue::Repeat { operand: op, .. }
        | Rvalue::Cast { operand: op, .. } => vec![op],
        Rvalue::Binary { lhs, rhs, .. }
        | Rvalue::Range {
            start: lhs,
            end: rhs,
            ..
        }
        | Rvalue::MakeSlice {
            start: lhs,
            len: rhs,
            ..
        } => vec![lhs, rhs],
        Rvalue::Struct { fields: ops, .. }
        | Rvalue::Array { items: ops, .. }
        | Rvalue::Call { args: ops, .. }
        | Rvalue::Intrinsic { args: ops, .. } => ops.iter().collect(),
        Rvalue::Ref { .. } => Vec::new(),
    };
    (operands.into_iter())
        .filter_map(|op| match op {
            Operand::Move(place) => Some(place),
            _ => None,
        })
        .collect()
}

/// How a statement changes what's there.
fn transfer(state: &mut State, kind: &StatementKind) {
    match kind {
        StatementKind::Assign(dest, rvalue) => {
            for moved in moves(rvalue) {
                if followed(moved) {
                    state.set(moved, Init::No);
                }
            }
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

impl FnBuilder<'_, '_> {
    /// A drop of `local`, if its type needs one; whether it's still there is worked out
    /// later.
    pub(super) fn drop_local(&mut self, local: Local, span: som_base::Span) {
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

    /// Ends `local`'s scope: drops it, then it stops existing.
    pub(super) fn end_local(&mut self, local: Local, span: som_base::Span) {
        self.drop_local(local, span.clone());
        self.push(StatementKind::StorageDead(local), span);
    }

    fn body_place_ty(&self, locals: &Arena<LocalDecl>, place: &Place) -> TypeId {
        let mut ty = locals[place.local].ty;
        for &projection in &place.projection {
            ty = (self.l.mir.project(ty, projection)).expect("lowered places are well-typed");
        }
        ty
    }

    /// Works out every drop against what's there along each path, as the module says.
    pub(super) fn elaborate_drops(&mut self, body: &mut Body) {
        // Temporaries still there when the function returns go then: those of a loop's
        // condition, say, which no statement ends.
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
                    body.blocks[block].stmts.push(Statement {
                        kind: StatementKind::Drop {
                            place: Place::local(temp),
                            glue,
                            flag: None,
                        },
                        span,
                    });
                }
            }
        }

        // What's there on entry to each block: parameters, and nothing else.
        let mut start = State(BTreeMap::new());
        for arg in body.args() {
            start.set(&Place::local(arg), Init::Yes);
        }
        let ids: Vec<BlockId> = body.blocks.iter_with_ids().map(|(id, _)| id).collect();
        let mut entry: HashMap<BlockId, State> = HashMap::from([(ids[0], start.clone())]);
        let mut work = vec![ids[0]];
        while let Some(id) = work.pop() {
            let mut state = entry[&id].clone();
            for stmt in &body.blocks[id].stmts {
                transfer(&mut state, &stmt.kind);
            }
            for next in body.blocks[id].terminator.kind.successors() {
                let changed = match entry.get_mut(&next) {
                    Some(existing) => existing.join(&state),
                    None => {
                        entry.insert(next, state.clone());
                        true
                    }
                };
                if changed && !work.contains(&next) {
                    work.push(next);
                }
            }
        }

        // Each drop as what's there says; and what an assignment replaces, dropped.
        let mut flags: HashMap<Place, Local> = HashMap::new();
        for &id in &ids {
            let Some(mut state) = entry.get(&id).cloned() else {
                // Never reached: nothing to drop.
                body.blocks[id]
                    .stmts
                    .retain(|s| !matches!(s.kind, StatementKind::Drop { .. }));
                continue;
            };
            let stmts = std::mem::take(&mut body.blocks[id].stmts);
            let mut out = Vec::new();
            for stmt in stmts {
                match stmt.kind {
                    StatementKind::Drop { place, .. } => {
                        self.drops_of(&mut body.locals, &state, &place, &mut flags, &stmt.span, &mut out);
                        if followed(&place) {
                            state.set(&place, Init::No);
                        }
                    }
                    StatementKind::Assign(dest, rvalue) => {
                        let ty = self.body_place_ty(&body.locals, &dest);
                        let replaced = self.l.drop_glue(ty).is_some()
                            && !body.outs.contains(&dest.local)
                            && dest.local != RETURN;
                        // After the value's worked out: it may read what's replaced.
                        let mut after_moves = state.clone();
                        for moved in moves(&rvalue) {
                            if followed(moved) {
                                after_moves.set(moved, Init::No);
                            }
                        }
                        let mut drops = Vec::new();
                        if replaced {
                            if followed(&dest) {
                                self.drops_of(&mut body.locals, &after_moves, &dest, &mut flags, &stmt.span, &mut drops);
                            } else if let Some(glue) = self.l.drop_glue(ty) {
                                drops.push(Statement {
                                    kind: StatementKind::Drop {
                                        place: dest.clone(),
                                        glue,
                                        flag: None,
                                    },
                                    span: stmt.span.clone(),
                                });
                            }
                        }
                        let kind = StatementKind::Assign(dest.clone(), rvalue);
                        transfer(&mut state, &kind);
                        if drops.is_empty() {
                            out.push(Statement {
                                kind,
                                span: stmt.span,
                            });
                            continue;
                        }
                        // `new = value; drop(dest); dest = new`.
                        let StatementKind::Assign(_, rvalue) = kind else { unreachable!() };
                        let new = body.locals.alloc(LocalDecl {
                            ty,
                            name: None,
                            mutable: false,
                            span: stmt.span.clone(),
                        });
                        out.push(Statement {
                            kind: StatementKind::Assign(Place::local(new), rvalue),
                            span: stmt.span.clone(),
                        });
                        out.extend(drops);
                        out.push(Statement {
                            kind: StatementKind::Assign(
                                dest,
                                Rvalue::Use(Operand::Move(Place::local(new))),
                            ),
                            span: stmt.span,
                        });
                    }
                    kind => {
                        transfer(&mut state, &kind);
                        out.push(Statement {
                            kind,
                            span: stmt.span,
                        });
                    }
                }
            }
            body.blocks[id].stmts = out;
        }
        if flags.is_empty() {
            return;
        }

        // Keep the flags up to date: set where what they follow is assigned, cleared where
        // it's moved out of or dropped.
        let bool = self.l.mir.types.bool;
        let set = |flag: Local, value: bool, span: &som_base::Span| Statement {
            kind: StatementKind::Assign(
                Place::local(flag),
                Rvalue::Use(Operand::Const(Const {
                    kind: ConstKind::Bool(value),
                    ty: bool,
                })),
            ),
            span: span.clone(),
        };
        for &id in &ids {
            let stmts = std::mem::take(&mut body.blocks[id].stmts);
            let mut out = Vec::new();
            for stmt in stmts {
                let mut updates = Vec::new();
                match &stmt.kind {
                    StatementKind::Assign(dest, rvalue) => {
                        for moved in moves(rvalue) {
                            for (place, &flag) in &flags {
                                if within(place, moved) || within(moved, place) {
                                    updates.push(set(flag, false, &stmt.span));
                                }
                            }
                        }
                        for (place, &flag) in &flags {
                            if followed(dest) && within(place, dest) {
                                updates.push(set(flag, true, &stmt.span));
                            }
                        }
                    }
                    StatementKind::Drop { place, .. } => {
                        for (followed_place, &flag) in &flags {
                            if within(followed_place, place) {
                                updates.push(set(flag, false, &stmt.span));
                            }
                        }
                    }
                    StatementKind::StorageDead(local) | StatementKind::StorageLive(local) => {
                        for (followed_place, &flag) in &flags {
                            if followed_place.local == *local {
                                updates.push(set(flag, false, &stmt.span));
                            }
                        }
                    }
                }
                out.push(stmt);
                out.extend(updates);
            }
            body.blocks[id].stmts = out;
        }
        // Parameters are there to begin with; everything else isn't.
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
    #[allow(clippy::too_many_arguments)]
    fn drops_of(
        &mut self,
        locals: &mut Arena<LocalDecl>,
        state: &State,
        place: &Place,
        flags: &mut HashMap<Place, Local>,
        span: &som_base::Span,
        out: &mut Vec<Statement>,
    ) {
        let ty = self.body_place_ty(locals, place);
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

fn drop_statement(place: Place, glue: FuncId, flag: Option<Local>, span: &som_base::Span) -> Statement {
    Statement {
        kind: StatementKind::Drop { place, glue, flag },
        span: span.clone(),
    }
}
