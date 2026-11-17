use som_ast::{BinOp, Expr, Id, Span, Stmt};
use som_mir::*;
use som_resolve::{DefId, DefKind};
use som_typeck::Value;

use super::FnBuilder;

impl FnBuilder<'_, '_> {
    /// A constant or a read of a place. Where a slice is expected, a reference to an array
    /// becomes one; where an optional is expected, a value is wrapped in one.
    pub(super) fn operand(&mut self, expr: Id<Expr>) -> Operand {
        let typed = self.typed.clone();
        if let Some((method, type_args)) = typed.conversion(expr) {
            return self.convert(expr, method, type_args.to_vec());
        }
        if !typed.is_wrapped(expr) && typed.sliced_to(expr).is_none() && !typed.is_downgraded(expr)
        {
            return self.operand_unwrapped(expr);
        }
        let rvalue = self.rvalue(expr);
        let ty = self.used_ty(expr);
        self.temp_read(ty, rvalue, self.span(expr))
    }

    /// `expr` passed to `From::from`, `method`.
    fn convert(&mut self, expr: Id<Expr>, method: DefId, type_args: Vec<TypeId>) -> Operand {
        let DefKind::Func(func) = self.l.names.def(method).kind else {
            unreachable!("methods are functions")
        };
        let span = self.span(expr);
        let array = self.operand_unwrapped(expr);
        let type_args = type_args.into_iter().map(|t| self.ty(t)).collect();
        let id = self.l.instance(func, type_args);
        let ty = self.l.mir.funcs[id].ret;
        let call = Rvalue::Call {
            func: id,
            args: vec![array],
        };
        self.temp_read(ty, call, span)
    }

    /// Like `operand`, but an enum whose variants hold nothing is the number its tag is:
    /// what `==` compares and `as` converts.
    pub(super) fn number(&mut self, expr: Id<Expr>) -> Operand {
        let ty = self.expr_ty(expr);
        // `self * self` of a `&i32`: the numbers it refers to.
        if self.typed.is_read_through(expr) {
            let Ty::Ref { to, .. } = *self.l.mir.types.get(ty) else {
                unreachable!("checked: only references are read through")
            };
            let place = self.place(expr).project(Projection::Deref);
            return self.read(place, to);
        }
        if !self.is_enum(ty) {
            return self.operand(expr);
        }
        let tag = self.place(expr).project(Projection::Field(0));
        // Makes sure the enum's copy, and so its tag, exists.
        self.place_ty(&tag);
        Operand::Copy(tag)
    }

    pub(super) fn is_enum(&self, ty: TypeId) -> bool {
        match self.l.mir.types.get(ty) {
            Ty::Struct(def, _) => matches!(
                self.l.names.def(*def).kind,
                DefKind::Struct(s) if self.l.ast.structs[s].is_enum()
            ),
            _ => false,
        }
    }

    /// Made a slice or wrapped in an optional where that's what's expected.
    pub(super) fn rvalue(&mut self, expr: Id<Expr>) -> Rvalue {
        let typed = self.typed.clone();
        if typed.conversion(expr).is_some() {
            return Rvalue::Use(self.operand(expr));
        }
        // A `&mut T` where a `&T` is expected: `&*r`, lending what it refers to.
        if typed.is_downgraded(expr) {
            let source = self.operand_unwrapped(expr);
            let place = self.into_place(source, expr);
            self.used_ty(expr);
            return Rvalue::Ref {
                mutable: false,
                place: place.project(Projection::Deref),
            };
        }
        let sliced = typed.sliced_to(expr).map(|ty| self.ty(ty));
        let value = match sliced {
            Some(slice) => {
                let span = self.span(expr);
                let source = self.operand_unwrapped(expr);
                // A `&mut` variable is lent, not moved: `&mut *r` becomes the slice.
                let source_ty = self.expr_ty(expr);
                let source = self.reborrow(source, source_ty, span);
                Rvalue::Cast {
                    operand: source,
                    ty: slice,
                }
            }
            None if typed.is_wrapped(expr) => {
                self.used_ty(expr);
                return Rvalue::Some(self.operand_unwrapped(expr));
            }
            None => return self.rvalue_unwrapped(expr),
        };
        if !typed.is_wrapped(expr) {
            return value;
        }
        let slice = sliced.expect("handled above");
        let temp = self.temp(slice, self.span(expr));
        self.assign(Place::local(temp), value, self.span(expr));
        self.used_ty(expr);
        Rvalue::Some(Operand::Move(Place::local(temp)))
    }

    /// The type `expr` has where it's used: made a slice, or wrapped in an optional.
    pub(super) fn used_ty(&mut self, expr: Id<Expr>) -> TypeId {
        let ty = match self.typed.sliced_to(expr) {
            Some(slice) => self.ty(slice),
            None => self.expr_ty(expr),
        };
        // `&mut T` used as a `&T`.
        let ty = match *self.l.mir.types.get(ty) {
            Ty::Ref { mutable: true, to } if self.typed.is_downgraded(expr) => {
                self.l.mir.types.intern(Ty::Ref { mutable: false, to })
            }
            _ => ty,
        };
        if self.typed.is_wrapped(expr) {
            self.l.mir.types.intern(Ty::Optional(ty))
        } else {
            ty
        }
    }

    /// The place `operand` reads, or a temporary holding the constant `expr` gave.
    pub(super) fn into_place(&mut self, operand: Operand, expr: Id<Expr>) -> Place {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => place,
            constant => {
                let ty = self.expr_ty(expr);
                let span = self.span(expr);
                let temp = self.temp(ty, span.clone());
                self.assign(Place::local(temp), Rvalue::Use(constant), span);
                Place::local(temp)
            }
        }
    }

    pub(super) fn operand_unwrapped(&mut self, expr: Id<Expr>) -> Operand {
        let ast = self.l.ast;
        let ty = self.expr_ty(expr);
        let span = self.span(expr);

        match &ast.exprs[expr] {
            Expr::Int(n) => self.constant(ConstKind::Int(*n), ty),
            Expr::Float(x) => self.constant(ConstKind::Float(*x), ty),
            Expr::Bool(b) => self.constant(ConstKind::Bool(*b), ty),
            Expr::Str(s) => self.constant(ConstKind::Str(s.to_string()), ty),
            Expr::None => self.constant(ConstKind::Zeroed, ty),
            Expr::Hole => unreachable!("programs with parse errors are never lowered"),
            Expr::Type(_) => unreachable!("types are only compared in `@if`s, worked out"),
            // `@const f(x)`: like a constant's name, worked out while compiling if it runs code.
            Expr::InPlace(c) if som_resolve::const_runs_code(ast, self.l.names, *c) => {
                let init = self.l.init(*c, self.owner.clone());
                self.constant(ConstKind::Pending(init), ty)
            }
            Expr::InPlace(c) => self.operand(ast.consts[*c].value),
            // `usize::MAX`: a number, which C writes as the target's own for `usize`.
            Expr::Var(path) if let Some((_, value)) = path.int_limit() => {
                self.constant(ConstKind::Int(value), ty)
            }
            Expr::Var(_) => self.var(expr, ty, span),
            Expr::Field { .. } | Expr::Deref(_) | Expr::Index { .. } if !self.is_prop(expr) => {
                let place = self.place(expr);
                self.read_at(place, ty, span)
            }
            Expr::Assign { target, value } => {
                let value = self.operand(*value);
                let place = self.place(*target);
                self.assign(place, Rvalue::Use(value), span);
                let unit = self.l.mir.types.unit;
                self.constant(ConstKind::Unit, unit)
            }
            Expr::Block { stmts, value } => self.block(stmts, *value, ty, span),
            Expr::Binary {
                op: op @ (BinOp::And | BinOp::Or),
                lhs,
                rhs,
            } => self.short_circuit(*op, *lhs, *rhs, ty, span),
            Expr::Fallback { optional, fallback } => self.fallback(*optional, *fallback, ty, span),
            // Over a generic enum's variants: the copy for this instance's `T`.
            Expr::Match { .. } if let Some(copy) = self.typed.chosen_match(expr) => {
                self.operand(copy)
            }
            Expr::Match { scrutinee, arms } => self.lower_match(expr, *scrutinee, arms),
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => self.conditional(*condition, *then, *otherwise, ty, span),
            _ => {
                let rvalue = self.rvalue_unwrapped(expr);
                self.temp_read(ty, rvalue, span)
            }
        }
    }

    fn var(&mut self, expr: Id<Expr>, ty: TypeId, span: Span) -> Operand {
        // Of functions sharing a name, the one the type checker picked.
        let def = (self.typed)
            .callee(expr, self.l.names)
            .expect("checked programs are fully resolved");
        match self.l.names.def(def).kind {
            DefKind::Func(func) => {
                let type_args = self.type_args(expr);
                let id = self.l.instance(func, type_args);
                self.constant(ConstKind::Func(id), ty)
            }
            // One that runs code is worked out while compiling, by its initializer.
            DefKind::Const(c) if som_resolve::const_runs_code(self.l.ast, self.l.names, c) => {
                let init = self.l.init(c, self.owner.clone());
                self.constant(ConstKind::Pending(init), ty)
            }
            // A plain constant stands for its value, made where it's used.
            DefKind::Const(c) => self.operand(self.l.ast.consts[c].value),
            // `Shape::Empty`
            DefKind::Variant { index, .. } => {
                let value = Rvalue::Struct {
                    ty,
                    variant: Some(index),
                    fields: Vec::new(),
                };
                self.temp_read(ty, value, span)
            }
            _ if self.l.names.is_value_param(def) => {
                let kind = match self.value_param(def) {
                    Value::Int(n) => ConstKind::Int(n as i128),
                    Value::Bool(b) => ConstKind::Bool(b),
                };
                self.constant(kind, ty)
            }
            _ if self.typed.is_narrowed(expr) => {
                let place = self.place(expr);
                self.read_at(place, ty, span)
            }
            _ => {
                let place = self.variable(def).expect("variables are bound");
                self.read_at(place, ty, span)
            }
        }
    }

    fn block(
        &mut self,
        stmts: &[Id<Stmt>],
        value: Option<Id<Expr>>,
        ty: TypeId,
        span: Span,
    ) -> Operand {
        self.in_scope(span.clone(), |b| {
            stmts.iter().for_each(|&s| b.stmt(s));
            let unit = b.l.mir.types.unit;
            let result = match value {
                Some(value) => b.operand(value),
                None if ty == unit => b.constant(ConstKind::Unit, ty),
                // Of another type, it never gets to its end: `else { return none; }`.
                // Nothing reads its value, so zeroes stand in.
                None => b.constant(ConstKind::Zeroed, ty),
            };
            // Locals die at the end of this scope, so take the value out of them first.
            let declares_locals = !b.scopes.last().expect("in scope").is_empty();
            match result {
                Operand::Copy(_) | Operand::Move(_) if declares_locals => {
                    b.temp_read(ty, Rvalue::Use(result), span.clone())
                }
                result => result,
            }
        })
    }

    /// `and` and `or`: the right side only runs when it decides the result.
    fn short_circuit(
        &mut self,
        op: BinOp,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
        ty: TypeId,
        span: Span,
    ) -> Operand {
        let result = self.temp(ty, span.clone());
        let left = self.condition(lhs);
        let (right_block, short_block, join) =
            (self.new_block(), self.new_block(), self.new_block());
        let (then, otherwise) = match op {
            BinOp::And => (right_block, short_block),
            _ => (short_block, right_block),
        };
        self.terminate(
            TerminatorKind::Branch {
                condition: left,
                then,
                otherwise,
            },
            span.clone(),
        );
        self.current = right_block;
        let right = self.condition(rhs);
        self.assign(Place::local(result), Rvalue::Use(right), span.clone());
        self.terminate(TerminatorKind::Goto(join), span.clone());
        // `false and …` is false, `true or …` is true.
        self.current = short_block;
        let known = self.constant(ConstKind::Bool(op == BinOp::Or), ty);
        self.assign(Place::local(result), Rvalue::Use(known), span.clone());
        self.terminate(TerminatorKind::Goto(join), span);
        self.current = join;
        self.read(Place::local(result), ty)
    }

    /// `optional else fallback`: the fallback is only evaluated when it's needed.
    fn fallback(
        &mut self,
        optional: Id<Expr>,
        fallback: Id<Expr>,
        ty: TypeId,
        span: Span,
    ) -> Operand {
        let result = self.temp(ty, span.clone());
        let (value, some_block, none_block) = self.branch_on_some(optional, ty, span.clone());
        let join = self.new_block();
        self.current = some_block;
        self.assign(Place::local(result), Rvalue::Use(value), span.clone());
        self.terminate(TerminatorKind::Goto(join), span.clone());
        self.current = none_block;
        let value = self.operand(fallback);
        self.assign(Place::local(result), Rvalue::Use(value), span.clone());
        self.terminate(TerminatorKind::Goto(join), span);
        self.current = join;
        self.read(Place::local(result), ty)
    }

    fn conditional(
        &mut self,
        condition: Id<Expr>,
        then: Id<Expr>,
        otherwise: Id<Expr>,
        ty: TypeId,
        span: Span,
    ) -> Operand {
        let result = self.temp(ty, span.clone());
        let condition = self.condition(condition);
        let (then_block, else_block, join) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then: then_block,
                otherwise: else_block,
            },
            span.clone(),
        );
        for (block, branch) in [(then_block, then), (else_block, otherwise)] {
            self.current = block;
            let value = self.operand(branch);
            // A branch that never finishes (`{ return 0; }`) doesn't get to the end.
            if self.typed.diverges(branch) {
                self.terminate(TerminatorKind::Unreachable, span.clone());
                continue;
            }
            self.assign(Place::local(result), Rvalue::Use(value), span.clone());
            self.terminate(TerminatorKind::Goto(join), span.clone());
        }
        self.current = join;
        self.read(Place::local(result), ty)
    }
}
