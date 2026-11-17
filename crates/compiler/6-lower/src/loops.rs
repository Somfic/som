use som_ast::{BinOp, Expr, Id, Ident, Span, Stmt};
use som_mir::*;
use som_resolve::DefKind;
use som_typeck::{Loop, Take, Value};

use super::{FnBuilder, LoopTargets};

/// The local viewing what a `for` goes through: named, so its borrow lasts the whole loop.
const VIEW_NAME: &str = "iterated";

/// What a counted `for` goes through: a counter from `first` up to `limit`, picking from
/// `elements` if it goes through an array or slice.
struct Counted {
    ty: TypeId,
    first: Operand,
    limit: Operand,
    elements: Option<Place>,
}

impl FnBuilder<'_, '_> {
    pub(super) fn loop_stmt(&mut self, body: &[Id<Stmt>], span: Span) {
        let (start, exit) = (self.new_block(), self.new_block());
        self.terminate(TerminatorKind::Goto(start), span.clone());
        self.current = start;
        self.in_loop(exit, start, None, |b| b.stmts(body, span.clone()));
        self.terminate(TerminatorKind::Goto(start), span);
        // Only a `break` gets here.
        self.current = exit;
    }

    pub(super) fn while_stmt(&mut self, condition: Id<Expr>, body: &[Id<Stmt>], span: Span) {
        let (header, body_block, exit) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(TerminatorKind::Goto(header), span.clone());
        self.current = header;
        let condition = self.operand(condition);
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then: body_block,
                otherwise: exit,
            },
            span.clone(),
        );
        self.current = body_block;
        self.in_loop(exit, header, None, |b| b.stmts(body, span.clone()));
        self.terminate(TerminatorKind::Goto(header), span);
        self.current = exit;
    }

    /// To the innermost loop, or with `named`, to the `for` loop whose variable it names.
    pub(super) fn break_or_continue(
        &mut self,
        id: Id<Stmt>,
        named: bool,
        is_break: bool,
        span: Span,
    ) {
        let targets = if named {
            let def = (self.l.names.loop_target(id)).expect("resolved: it names a loop");
            let DefKind::Local(for_stmt) = self.l.names.def(def).kind else {
                unreachable!("resolved: loop variables are locals")
            };
            *(self.loops.iter().rev())
                .find(|l| l.stmt == Some(for_stmt))
                .expect("resolved: the loop it names is around it")
        } else {
            *self.loops.last().expect("checked: break is in a loop")
        };
        // As they would at the end of the body.
        self.end_scopes_from(targets.depth, span.clone());
        let target = if is_break {
            targets.exit
        } else {
            targets.next
        };
        self.terminate(TerminatorKind::Goto(target), span);
        // Whatever follows in the body can't be reached.
        self.current = self.new_block();
    }

    /// Runs `body` with `break` going to `exit` and `continue` to `next`.
    pub(super) fn in_loop(
        &mut self,
        exit: BlockId,
        next: BlockId,
        stmt: Option<Id<Stmt>>,
        body: impl FnOnce(&mut Self),
    ) {
        self.loops.push(LoopTargets {
            exit,
            next,
            depth: self.scopes.len(),
            stmt,
        });
        body(self);
        self.loops.pop();
    }

    /// A `for` loop's variable, and its type.
    pub(super) fn loop_variable(&mut self, stmt: Id<Stmt>) -> (DefId, TypeId) {
        let def = (self.l.names.local(stmt)).expect("every for loop is resolved");
        let ty = self.typed.local(def);
        (def, self.ty(ty))
    }

    /// A counter going up to a limit, both worked out once before the loop; for an array or
    /// slice, a view of it, which keeps it borrowed until the loop ends.
    pub(super) fn for_loop(
        &mut self,
        stmt: Id<Stmt>,
        name: &Ident,
        mutable: bool,
        iterable: Id<Expr>,
        body: &[Id<Stmt>],
        span: Span,
    ) {
        let kind = (self.typed.for_loop(stmt)).expect("checked: for loops go through something");
        let (def, value_ty) = self.loop_variable(stmt);
        if let Loop::Iterator { start, copied } = kind {
            return self.iterator_loop(stmt, name, mutable, iterable, body, span, start, copied);
        }
        self.in_scope(span.clone(), |b| {
            let counted = match kind {
                Loop::Range => b.range_bounds(iterable),
                Loop::Elements { take } => b.elements_bounds(iterable, take, span.clone()),
                Loop::Iterator { .. } => unreachable!("lowered as an iterator loop above"),
            };
            let elements = counted.elements.clone();
            b.counted_loop(stmt, counted, span.clone(), |b, counter| {
                b.in_scope(span.clone(), |b| {
                    let local = b.let_local(def, name, value_ty, mutable, span.clone());
                    let element = |elements: &Place| {
                        elements.clone().project(Projection::Index(counter))
                    };
                    let value = match (&elements, kind) {
                        (None, _) => Rvalue::Use(Operand::Copy(Place::local(counter))),
                        (
                            Some(elements),
                            Loop::Elements {
                                take: take @ (Take::Ref | Take::RefMut),
                            },
                        ) => Rvalue::Ref {
                            mutable: take == Take::RefMut,
                            place: element(elements),
                        },
                        (Some(elements), _) => Rvalue::Use(Operand::Copy(element(elements))),
                    };
                    b.assign(Place::local(local), value, span.clone());
                    b.stmts(body, span.clone());
                });
            });
        });
    }

    fn range_bounds(&mut self, range: Id<Expr>) -> Counted {
        let range_ty = self.expr_ty(range);
        let Ty::Range(int) = *self.l.mir.types.get(range_ty) else {
            unreachable!("checked: ranges are looped over as ranges")
        };
        let (first, limit) = match &self.l.ast.exprs[range] {
            Expr::Range {
                start: Some(start),
                end: Some(end),
            } => (self.operand(*start), self.operand(*end)),
            _ => {
                let place = self.place(range);
                let field = |i| Operand::Copy(place.clone().project(Projection::Field(i)));
                (field(0), field(1))
            }
        };
        Counted {
            ty: int,
            first,
            limit,
            elements: None,
        }
    }

    fn elements_bounds(&mut self, iterable: Id<Expr>, take: Take, span: Span) -> Counted {
        let usize = self.l.mir.types.usize;
        let iterable_ty = self.expr_ty(iterable);
        let mut place = self.place(iterable);
        // A temporary holding what's gone through, like a call's result, ends with the loop,
        // and so do the borrows it holds.
        if place.projection.is_empty()
            && place.local != RETURN
            && self.locals[place.local].name.is_none()
        {
            self.scopes.last_mut().expect("in scope").push(place.local);
        }
        let mut viewed_ty = iterable_ty;
        if let Ty::Ref { to, .. } = *self.l.mir.types.get(iterable_ty) {
            place = place.project(Projection::Deref);
            viewed_ty = to;
        }
        // Borrowed to change, for references to change the elements.
        let to_change = take == Take::RefMut;
        let view_ty = self.l.mir.types.intern(Ty::Ref {
            mutable: to_change,
            to: viewed_ty,
        });
        let view = self.new_local(view_ty, Some(VIEW_NAME.into()), false, span.clone());
        self.declare(view, span.clone());
        let borrow = Rvalue::Ref {
            mutable: to_change,
            place,
        };
        self.assign(Place::local(view), borrow, span);
        let limit = match *self.l.mir.types.get(viewed_ty) {
            Ty::Array { len, .. } => {
                let len = self.l.mir.types.len(len);
                self.constant(ConstKind::Int(len as i128), usize)
            }
            _ => Operand::Copy(Place::local(view).project(Projection::Field(1))),
        };
        Counted {
            ty: usize,
            first: self.constant(ConstKind::Int(0), usize),
            limit,
            elements: Some(Place::local(view).project(Projection::Deref)),
        }
    }

    /// Runs `body` with the counter for each value; `continue` goes on to the next one.
    fn counted_loop(
        &mut self,
        stmt: Id<Stmt>,
        counted: Counted,
        span: Span,
        body: impl FnOnce(&mut Self, Local),
    ) {
        let counter = self.temp(counted.ty, span.clone());
        let first = Rvalue::Use(counted.first);
        self.assign(Place::local(counter), first, span.clone());
        let stop = self.temp(counted.ty, span.clone());
        self.assign(Place::local(stop), Rvalue::Use(counted.limit), span.clone());

        let (header, body_block, step, exit) = (
            self.new_block(),
            self.new_block(),
            self.new_block(),
            self.new_block(),
        );
        self.terminate(TerminatorKind::Goto(header), span.clone());
        self.current = header;
        let more = self.temp(self.l.mir.types.bool, span.clone());
        let compare = Rvalue::Binary {
            op: BinOp::LessThan,
            lhs: Operand::Copy(Place::local(counter)),
            rhs: Operand::Copy(Place::local(stop)),
        };
        self.assign(Place::local(more), compare, span.clone());
        self.terminate(
            TerminatorKind::Branch {
                condition: Operand::Copy(Place::local(more)),
                then: body_block,
                otherwise: exit,
            },
            span.clone(),
        );

        self.current = body_block;
        self.in_loop(exit, step, Some(stmt), |b| body(b, counter));
        self.terminate(TerminatorKind::Goto(step), span.clone());

        self.current = step;
        let one = self.constant(ConstKind::Int(1), counted.ty);
        let next = Rvalue::Binary {
            op: BinOp::Add,
            lhs: Operand::Copy(Place::local(counter)),
            rhs: one,
        };
        self.assign(Place::local(counter), next, span.clone());
        self.terminate(TerminatorKind::Goto(header), span);
        self.current = exit;
    }

    /// `@for i in 0..4 { body }`: the body once for each value, with `i` that value. `break`
    /// leaves them all; `continue` goes on to the next copy.
    pub(super) fn settled_for(
        &mut self,
        stmt: Id<Stmt>,
        name: &Ident,
        mutable: bool,
        iterable: Id<Expr>,
        body: &[Id<Stmt>],
        span: Span,
    ) {
        let (def, value_ty) = self.loop_variable(stmt);
        // Each copy's value: a number of the range, or an element written out.
        let values: Vec<Result<i128, Id<Expr>>> = match &self.l.ast.exprs[iterable] {
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
            self.in_loop(exit, next, Some(stmt), |b| {
                b.in_scope(span.clone(), |b| {
                    let local = b.let_local(def, name, value_ty, mutable, span.clone());
                    let value = match value {
                        Ok(n) => Rvalue::Use(b.constant(ConstKind::Int(n), value_ty)),
                        Err(item) => b.rvalue(item),
                    };
                    b.assign(Place::local(local), value, span.clone());
                    b.stmts(body, span.clone());
                });
            });
            self.terminate(TerminatorKind::Goto(next), span.clone());
        }
        self.current = exit;
    }

    /// The number `expr` is in this instance, which the type checker made sure is known: a
    /// literal, a constant, or a value parameter, and arithmetic of those.
    fn settled_int(&mut self, expr: Id<Expr>) -> i128 {
        let ast = self.l.ast;
        if let Some(n) = self.typed.const_number(ast, self.l.names, expr) {
            return n;
        }
        match &ast.exprs[expr] {
            Expr::Var(_) => {
                let def = (self.l.names.expr(expr)).expect("checked programs are resolved");
                match self.value_param(def) {
                    Value::Int(n) => n as i128,
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
}
