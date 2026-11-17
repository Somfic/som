use som_ast::{BinOp, Expr, Id, Span, Stmt};
use som_mir::*;
use som_typeck::{Loop, Take, Value};

use super::{FnBuilder, LoopTargets};

impl FnBuilder<'_, '_> {
    /// `for name in iterable { body }`: a counter going up to a limit, both worked out once
    /// before the loop; and for an array or slice, a view of it, which keeps it borrowed until
    /// the loop ends.
    pub(super) fn for_loop(
        &mut self,
        stmt: Id<Stmt>,
        name: &som_ast::Ident,
        mutable: bool,
        iterable: Id<Expr>,
        body: &[Id<Stmt>],
        span: Span,
    ) {
        let ast = self.l.ast;
        let kind = self
            .typed
            .for_loop(stmt)
            .expect("checked: for loops go through something");
        let def = self
            .l
            .names
            .local(stmt)
            .expect("every for loop is resolved");
        let value_ty = self.typed.local(def);
        let value_ty = self.ty(value_ty);
        let bool = self.l.mir.types.bool;
        let usize = self.l.mir.types.intern(Ty::Int(IntTy::USize));
        // Through an iterator: a loop of its own.
        if let Loop::Iterator { start, copied } = kind {
            return self.iterator_loop(stmt, name, mutable, iterable, body, span, start, copied);
        }

        self.in_scope(span.clone(), |b| {
            // What to go through: a counter's type, where it starts, where it stops, and the
            // elements it picks.
            let (counter_ty, first, limit, elements) = match kind {
                Loop::Range => {
                    let range_ty = b.expr_ty(iterable);
                    let Ty::Range(int) = *b.l.mir.types.get(range_ty) else {
                        unreachable!("checked: ranges are looped over as ranges")
                    };
                    let (first, limit) = match &ast.exprs[iterable] {
                        Expr::Range {
                            start: Some(start),
                            end: Some(end),
                        } => (b.operand(*start), b.operand(*end)),
                        _ => {
                            let place = b.place(iterable);
                            let field =
                                |i| Operand::Copy(place.clone().project(Projection::Field(i)));
                            (field(0), field(1))
                        }
                    };
                    (int, first, limit, None)
                }
                Loop::Elements { take } => {
                    let iterable_ty = b.expr_ty(iterable);
                    let mut place = b.place(iterable);
                    // A temporary holding what's gone through, like a call's result, ends with
                    // the loop, and so do the borrows it holds.
                    if place.projection.is_empty()
                        && place.local != RETURN
                        && b.locals[place.local].name.is_none()
                    {
                        b.scopes.last_mut().expect("in scope").push(place.local);
                    }
                    let mut viewed_ty = iterable_ty;
                    if let Ty::Ref { to, .. } = *b.l.mir.types.get(iterable_ty) {
                        place = place.project(Projection::Deref);
                        viewed_ty = to;
                    }
                    // Borrowed to change, for references to change the elements.
                    let to_change = take == Take::RefMut;
                    let view_ty = b.l.mir.types.intern(Ty::Ref {
                        mutable: to_change,
                        to: viewed_ty,
                    });
                    // Named, so the borrow it holds lasts the whole loop.
                    let view = b.new_local(view_ty, Some("iterated".into()), false, span.clone());
                    b.scopes.last_mut().expect("in scope").push(view);
                    b.push(StatementKind::StorageLive(view), span.clone());
                    let borrow = Rvalue::Ref {
                        mutable: to_change,
                        place,
                    };
                    b.assign(Place::local(view), borrow, span.clone());
                    let limit = match *b.l.mir.types.get(viewed_ty) {
                        Ty::Array { len, .. } => {
                            let len = b.l.mir.types.len(len);
                            b.constant(ConstKind::Int(len as i128), usize)
                        }
                        _ => Operand::Copy(Place::local(view).project(Projection::Field(1))),
                    };
                    let first = b.constant(ConstKind::Int(0), usize);
                    let elements = Place::local(view).project(Projection::Deref);
                    (usize, first, limit, Some(elements))
                }
                Loop::Iterator { .. } => unreachable!("lowered as an iterator loop above"),
            };
            let counter = b.temp(counter_ty, span.clone());
            b.assign(Place::local(counter), Rvalue::Use(first), span.clone());
            let stop = b.temp(counter_ty, span.clone());
            b.assign(Place::local(stop), Rvalue::Use(limit), span.clone());

            let (header, body_block, step, exit) =
                (b.new_block(), b.new_block(), b.new_block(), b.new_block());
            b.terminate(TerminatorKind::Goto(header), span.clone());
            b.current = header;
            let more = b.temp(bool, span.clone());
            let compare = Rvalue::Binary {
                op: BinOp::LessThan,
                lhs: Operand::Copy(Place::local(counter)),
                rhs: Operand::Copy(Place::local(stop)),
            };
            b.assign(Place::local(more), compare, span.clone());
            b.terminate(
                TerminatorKind::Branch {
                    condition: Operand::Copy(Place::local(more)),
                    then: body_block,
                    otherwise: exit,
                },
                span.clone(),
            );

            // Each time round: the value, then the body. `break` leaves both.
            b.current = body_block;
            // `continue` goes on to the next value.
            b.loops.push(LoopTargets {
                exit,
                next: step,
                depth: b.scopes.len(),
                stmt: Some(stmt),
            });
            b.in_scope(span.clone(), |b| {
                let local = b.new_local(
                    value_ty,
                    Some(name.name.to_string()),
                    mutable,
                    name.span.clone(),
                );
                b.bindings.insert(def, local);
                b.scopes.last_mut().expect("in scope").push(local);
                b.push(StatementKind::StorageLive(local), span.clone());
                let value = match (&elements, kind) {
                    (None, _) => Rvalue::Use(Operand::Copy(Place::local(counter))),
                    (
                        Some(elements),
                        Loop::Elements {
                            take: take @ (Take::Ref | Take::RefMut),
                        },
                    ) => Rvalue::Ref {
                        mutable: take == Take::RefMut,
                        place: elements.clone().project(Projection::Index(counter)),
                    },
                    (Some(elements), _) => Rvalue::Use(Operand::Copy(
                        elements.clone().project(Projection::Index(counter)),
                    )),
                };
                b.assign(Place::local(local), value, span.clone());
                b.stmts(body, span.clone());
            });
            b.loops.pop();
            b.terminate(TerminatorKind::Goto(step), span.clone());

            b.current = step;
            let one = b.constant(ConstKind::Int(1), counter_ty);
            let next = Rvalue::Binary {
                op: BinOp::Add,
                lhs: Operand::Copy(Place::local(counter)),
                rhs: one,
            };
            b.assign(Place::local(counter), next, span.clone());
            b.terminate(TerminatorKind::Goto(header), span.clone());
            b.current = exit;
        });
    }

    /// A loop's body, where `break` goes to `exit` and `continue` to `next`.
    /// `@for i in 0..4 { body }`: the body once for each value, with `i` that value. `break`
    /// leaves them all; `continue` goes on to the next copy.
    pub(super) fn settled_for(
        &mut self,
        stmt: Id<Stmt>,
        name: &som_ast::Ident,
        mutable: bool,
        iterable: Id<Expr>,
        body: &[Id<Stmt>],
        span: Span,
    ) {
        let ast = self.l.ast;
        let def = (self.l.names.local(stmt)).expect("every for loop is resolved");
        let value_ty = self.typed.local(def);
        let value_ty = self.ty(value_ty);
        // Each copy's value: a number of the range, or an element written out.
        let values: Vec<Result<i128, Id<Expr>>> = match &ast.exprs[iterable] {
            Expr::Range {
                start: Some(start),
                end: Some(end),
            } => (self.settled_int(*start)..self.settled_int(*end))
                .map(Ok)
                .collect(),
            Expr::Array(items) => items.iter().map(|&item| Err(item)).collect(),
            _ => unreachable!("checked: an `@for` loops over a range or an array written out"),
        };
        let exit = self.new_block();
        let starts: Vec<BlockId> = values.iter().map(|_| self.new_block()).collect();
        let first = starts.first().copied().unwrap_or(exit);
        self.terminate(TerminatorKind::Goto(first), span.clone());
        for (k, value) in values.into_iter().enumerate() {
            self.current = starts[k];
            let next = starts.get(k + 1).copied().unwrap_or(exit);
            self.loops.push(LoopTargets {
                exit,
                next,
                depth: self.scopes.len(),
                stmt: Some(stmt),
            });
            self.in_scope(span.clone(), |b| {
                let local = b.new_local(
                    value_ty,
                    Some(name.name.to_string()),
                    mutable,
                    name.span.clone(),
                );
                b.bindings.insert(def, local);
                b.scopes.last_mut().expect("in scope").push(local);
                b.push(StatementKind::StorageLive(local), span.clone());
                let value = match value {
                    Ok(n) => Rvalue::Use(b.constant(ConstKind::Int(n), value_ty)),
                    Err(item) => b.rvalue(item),
                };
                b.assign(Place::local(local), value, span.clone());
                b.stmts(body, span.clone());
            });
            self.loops.pop();
            self.terminate(TerminatorKind::Goto(next), span.clone());
        }
        self.current = exit;
    }

    /// The number `expr` is in this copy of the function, which the type checker made sure is
    /// known: a literal, a constant, or a value parameter, and arithmetic of those.
    pub(super) fn settled_int(&mut self, expr: Id<Expr>) -> i128 {
        let ast = self.l.ast;
        if let Some(n) = self.typed.const_number(ast, self.l.names, expr) {
            return n;
        }
        match &ast.exprs[expr] {
            Expr::Var(_) => {
                let def = (self.l.names.expr(expr)).expect("checked programs are resolved");
                let param = self.l.mir.types.intern(Ty::Param(def));
                let value = self.value_of(param);
                match *self.l.mir.types.get(value) {
                    Ty::Value(Value::Int(n)) => n as i128,
                    _ => unreachable!("checked: a value parameter that's a number"),
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let (a, b) = (self.settled_int(*lhs), self.settled_int(*rhs));
                match op {
                    BinOp::Add => a + b,
                    BinOp::Subtract => a - b,
                    BinOp::Multiply => a * b,
                    BinOp::Divide if b != 0 => a / b,
                    BinOp::Modulo if b != 0 => a % b,
                    _ => unreachable!("checked: arithmetic of known numbers"),
                }
            }
            Expr::Cast { value, .. } => self.settled_int(*value),
            Expr::Neg(value) => -self.settled_int(*value),
            _ => unreachable!("checked: a number known while compiling"),
        }
    }

    pub(super) fn loop_body(
        &mut self,
        body: &[Id<Stmt>],
        exit: BlockId,
        next: BlockId,
        span: Span,
    ) {
        self.loops.push(LoopTargets {
            exit,
            next,
            depth: self.scopes.len(),
            stmt: None,
        });
        self.stmts(body, span);
        self.loops.pop();
    }
}
