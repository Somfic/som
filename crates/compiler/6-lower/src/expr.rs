use som_ast::{BinOp, Expr, Id, Span, Struct};
use som_mir::*;
use som_resolve::{DefId, DefKind};
use som_typeck::{PointerMethod, Value};

use super::FnBuilder;

impl FnBuilder<'_, '_> {
    /// Evaluates `expr` to an operand: a constant or a read of a place. Where a slice is
    /// expected, a reference to an array becomes one; where an optional is expected, a value
    /// is wrapped in one.
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
        let temp = self.temp(ty, self.span(expr));
        self.assign(Place::local(temp), rvalue, self.span(expr));
        self.read(Place::local(temp), ty)
    }

    /// `expr` made into a struct by `From`'s `from`, `method`, with type arguments
    /// `type_args`: the value, passed to it.
    pub(super) fn convert(
        &mut self,
        expr: Id<Expr>,
        method: DefId,
        type_args: Vec<TypeId>,
    ) -> Operand {
        let DefKind::Func(func) = self.l.names.def(method).kind else {
            unreachable!("methods are functions")
        };
        let span = self.span(expr);
        let array = self.operand_unwrapped(expr);
        let type_args = type_args.into_iter().map(|t| self.ty(t)).collect();
        let id = self.l.instance(func, type_args);
        let ty = self.l.mir.funcs[id].ret;
        let temp = self.temp(ty, span.clone());
        let call = Rvalue::Call {
            func: id,
            args: vec![array],
        };
        self.assign(Place::local(temp), call, span);
        self.read(Place::local(temp), ty)
    }

    /// Evaluates `expr` like `operand`, but an enum whose variants hold nothing as the number
    /// its tag is: what `==` compares and `as` converts.
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
        let is_enum = match self.l.mir.types.get(ty) {
            Ty::Struct(def, _) => matches!(
                self.l.names.def(*def).kind,
                DefKind::Struct(s) if self.l.ast.structs[s].is_enum()
            ),
            _ => false,
        };
        if !is_enum {
            return self.operand(expr);
        }
        let tag = self.place(expr).project(Projection::Field(0));
        // Make sure the enum's copy, and so its tag, exists.
        self.place_ty(&tag);
        Operand::Copy(tag)
    }

    /// Evaluates `expr` to a value that can be assigned to a place, made a slice or wrapped in
    /// an optional where that's what's expected.
    pub(super) fn rvalue(&mut self, expr: Id<Expr>) -> Rvalue {
        let typed = self.typed.clone();
        if typed.conversion(expr).is_some() {
            return Rvalue::Use(self.operand(expr));
        }
        // A `&mut T` where a `&T` is expected: `&*r`, lending what it refers to.
        if typed.is_downgraded(expr) {
            let span = self.span(expr);
            let source = self.operand_unwrapped(expr);
            let place = match source {
                Operand::Move(place) | Operand::Copy(place) => place,
                constant => {
                    let ty = self.expr_ty(expr);
                    let temp = self.temp(ty, span.clone());
                    self.assign(Place::local(temp), Rvalue::Use(constant), span);
                    Place::local(temp)
                }
            };
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
            Expr::Var(_) => {
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
                    DefKind::Const(c)
                        if som_resolve::const_runs_code(self.l.ast, self.l.names, c) =>
                    {
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
                        let temp = self.temp(ty, span.clone());
                        self.assign(Place::local(temp), value, span);
                        self.read(Place::local(temp), ty)
                    }
                    // A value parameter: what it is in this copy of the function.
                    _ if self.l.names.is_value_param(def) => {
                        let param = self.l.mir.types.intern(Ty::Param(def));
                        let value = self.value_of(param);
                        let kind = match *self.l.mir.types.get(value) {
                            Ty::Value(Value::Int(n)) => ConstKind::Int(n as i128),
                            Ty::Value(Value::Bool(b)) => ConstKind::Bool(b),
                            _ => unreachable!("value parameters stand for values"),
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

            Expr::Block { stmts, value } => self.in_scope(span.clone(), |b| {
                stmts.iter().for_each(|&s| b.stmt(s));
                let unit = b.l.mir.types.unit;
                let result = match value {
                    Some(value) => b.operand(*value),
                    None if ty == unit => b.constant(ConstKind::Unit, ty),
                    // Of another type, it never gets to its end: `else { return none; }`.
                    // Nothing reads its value, so zeroes stand in.
                    None => b.constant(ConstKind::Zeroed, ty),
                };
                // Locals die at the end of this scope, so take the value out of them first.
                let declares_locals = !b.scopes.last().expect("in scope").is_empty();
                match result {
                    Operand::Copy(_) | Operand::Move(_) if declares_locals => {
                        let temp = b.temp(ty, span.clone());
                        b.assign(Place::local(temp), Rvalue::Use(result), span.clone());
                        b.read(Place::local(temp), ty)
                    }
                    result => result,
                }
            }),

            // The right side only runs when it decides the result.
            Expr::Binary {
                op: op @ (BinOp::And | BinOp::Or),
                lhs,
                rhs,
            } => {
                let result = self.temp(ty, span.clone());
                let left = self.condition(*lhs);
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
                let right = self.condition(*rhs);
                self.assign(Place::local(result), Rvalue::Use(right), span.clone());
                self.terminate(TerminatorKind::Goto(join), span.clone());
                // `false and …` is false, `true or …` is true.
                self.current = short_block;
                let known = self.constant(ConstKind::Bool(*op == BinOp::Or), ty);
                self.assign(Place::local(result), Rvalue::Use(known), span.clone());
                self.terminate(TerminatorKind::Goto(join), span);
                self.current = join;
                self.read(Place::local(result), ty)
            }

            Expr::Fallback { optional, fallback } => {
                let result = self.temp(ty, span.clone());
                let (value, some_block, none_block) =
                    self.branch_on_some(*optional, ty, span.clone());
                let join = self.new_block();
                self.current = some_block;
                self.assign(Place::local(result), Rvalue::Use(value), span.clone());
                self.terminate(TerminatorKind::Goto(join), span.clone());
                // The fallback is only evaluated when it's needed.
                self.current = none_block;
                let value = self.operand(*fallback);
                self.assign(Place::local(result), Rvalue::Use(value), span.clone());
                self.terminate(TerminatorKind::Goto(join), span);
                self.current = join;
                self.read(Place::local(result), ty)
            }

            // Over a generic enum's variants: the copy for this instance's `T`.
            Expr::Match { .. } if let Some(copy) = self.typed.chosen_match(expr) => {
                return self.operand(copy);
            }
            Expr::Match { scrutinee, arms } => self.lower_match(expr, *scrutinee, arms),

            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                let result = self.temp(ty, span.clone());
                let condition = self.condition(*condition);
                let (then_block, else_block, join) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.terminate(
                    TerminatorKind::Branch {
                        condition,
                        then: then_block,
                        otherwise: else_block,
                    },
                    span.clone(),
                );
                for (block, branch) in [(then_block, *then), (else_block, *otherwise)] {
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

            _ => {
                let rvalue = self.rvalue_unwrapped(expr);
                let temp = self.temp(ty, span.clone());
                self.assign(Place::local(temp), rvalue, span);
                self.read(Place::local(temp), ty)
            }
        }
    }

    pub(super) fn rvalue_unwrapped(&mut self, expr: Id<Expr>) -> Rvalue {
        let ast = self.l.ast;
        match &ast.exprs[expr] {
            Expr::Interpolated(parts) => self.interpolated(expr, parts),
            Expr::Binary {
                op: BinOp::And | BinOp::Or,
                ..
            } => Rvalue::Use(self.operand_unwrapped(expr)),
            // `x != none` is whether `x` holds a value.
            Expr::Binary { op, lhs, rhs }
                if matches!(op, BinOp::Equals | BinOp::NotEquals)
                    && (matches!(ast.exprs[*lhs], Expr::None)
                        || matches!(ast.exprs[*rhs], Expr::None)) =>
            {
                let optional = if matches!(ast.exprs[*lhs], Expr::None) {
                    *rhs
                } else {
                    *lhs
                };
                let place = self.place(optional);
                let holds_value = Operand::Copy(place.project(Projection::Field(0)));
                match op {
                    BinOp::NotEquals => Rvalue::Use(holds_value),
                    _ => Rvalue::Not(holds_value),
                }
            }
            Expr::Binary {
                op: op @ (BinOp::Equals | BinOp::NotEquals),
                lhs,
                rhs,
            } if self.is_optional(*lhs) || self.is_optional(*rhs) => {
                let equal = self.optional_equality(*lhs, *rhs, self.span(expr));
                match op {
                    BinOp::Equals => Rvalue::Use(equal),
                    _ => Rvalue::Not(equal),
                }
            }
            // Values of a type implementing `Eq` compare with its `eq`.
            Expr::Binary { op, lhs, rhs }
                if let Some(lang) = self.typed.operator_trait(expr)
                    && let Some(call) =
                        self.operator_call(expr, lang, *op, *lhs, *rhs, self.span(expr)) =>
            {
                call
            }
            Expr::Binary { op, lhs, rhs } => Rvalue::Binary {
                op: *op,
                lhs: self.number(*lhs),
                rhs: self.number(*rhs),
            },
            Expr::Not(inner) => Rvalue::Not(self.condition(*inner)),
            // `~x`: `Not` of an integer flips its bits.
            Expr::BitNot(inner) => Rvalue::Not(self.number(*inner)),
            // `@unreachable` and `@fail(message)`: the program's `@on fail` functions are told,
            // then it stops. Nothing after runs, so the value is never read: zeroes stand in.
            Expr::Unreachable | Expr::Fail(_) => {
                let span = self.span(expr);
                let message = match ast.exprs[expr] {
                    Expr::Fail(message) => Some(self.operand(message)),
                    _ => None,
                };
                self.fail(message, span);
                let ty = self.expr_ty(expr);
                Rvalue::Use(self.constant(ConstKind::Zeroed, ty))
            }
            // `-x` is `0 - x`.
            Expr::Neg(inner) => {
                let ty = self.expr_ty(*inner);
                let zero = match self.l.mir.types.get(ty) {
                    Ty::Float(_) => ConstKind::Float(0.0),
                    _ => ConstKind::Int(0),
                };
                Rvalue::Binary {
                    op: BinOp::Subtract,
                    lhs: self.constant(zero, ty),
                    rhs: self.operand(*inner),
                }
            }
            Expr::Borrow {
                mutable,
                expr: inner,
            } => {
                // Make sure the reference type exists in MIR's table, even after substitution.
                self.expr_ty(expr);
                Rvalue::Ref {
                    mutable: *mutable,
                    place: self.place(*inner),
                }
            }
            // A variable living to the end of the surrounding scope, like a `let` before the
            // call. It starts zeroed: C functions don't always fill in their out-parameters.
            Expr::Out(name) => {
                let def = self
                    .l
                    .names
                    .expr(expr)
                    .expect("checked programs are fully resolved");
                let ty = self.ty(self.typed.local(def));
                let span = name.span.clone();
                let local = self.new_local(ty, Some(name.name.to_string()), true, span.clone());
                self.bindings.insert(def, local);
                self.scopes
                    .last_mut()
                    .expect("arguments are inside a scope")
                    .push(local);
                self.push(StatementKind::StorageLive(local), span.clone());
                let zero = self.constant(ConstKind::Zeroed, ty);
                self.assign(Place::local(local), Rvalue::Use(zero), span);
                // Make sure the reference type exists in MIR's table, even after substitution.
                self.expr_ty(expr);
                Rvalue::Ref {
                    mutable: true,
                    place: Place::local(local),
                }
            }
            Expr::Construct { fields, .. } => {
                let def = self
                    .l
                    .names
                    .expr(expr)
                    .expect("checked programs are fully resolved");
                let (declared, variant) = match self.l.names.def(def).kind {
                    DefKind::Struct(s) => (&ast.structs[s].fields, None),
                    // `Shape::Rect { w: 1.0, h: 2.0 }`
                    DefKind::Variant { strukt, index } => {
                        (&ast.structs[strukt].variants()[index].fields, Some(index))
                    }
                    _ => unreachable!("constructors name structs and variants"),
                };
                // Evaluate in source order, then put the values in declaration order.
                let mut values: Vec<Option<Operand>> = vec![None; declared.len()];
                for (name, value) in fields {
                    let index = declared
                        .iter()
                        .position(|f| f.name.name == name.name)
                        .expect("checked field");
                    values[index] = Some(self.operand(*value));
                }
                let ty = self.expr_ty(expr);
                Rvalue::Struct {
                    ty,
                    variant,
                    fields: values
                        .into_iter()
                        .map(|v| v.expect("checked: no missing fields"))
                        .collect(),
                }
            }
            Expr::Call { args, .. } => {
                // An instance it asks for is first used here.
                self.l.at = Some(self.span(expr));
                let def = (self.typed)
                    .callee(expr, self.l.names)
                    .expect("checked programs are fully resolved");
                match self.l.names.def(def).kind {
                    DefKind::Func(func) => self.call(expr, func, None, args),
                    // `Shape::Circle(1.0)`
                    DefKind::Variant { index, .. } => {
                        let fields = args.iter().map(|&a| self.operand(a)).collect();
                        let ty = self.expr_ty(expr);
                        Rvalue::Struct {
                            ty,
                            variant: Some(index),
                            fields,
                        }
                    }
                    _ => unreachable!("calls name functions and variants"),
                }
            }
            Expr::Array(items) => {
                let items: Vec<Operand> = items.iter().map(|&item| self.operand(item)).collect();
                let len = Ty::Value(Value::Int(items.len() as u128));
                let len = self.l.mir.types.intern(len);
                self.collection(expr, len, |ty| Rvalue::Array { ty, items })
            }
            Expr::Cast { value, .. } => {
                let ty = self.expr_ty(expr);
                let from = self.expr_ty(*value);
                // A slice as a pointer: where its elements start.
                if let Ty::Ref { to, .. } = *self.l.mir.types.get(from)
                    && matches!(self.l.mir.types.get(to), Ty::Slice(_) | Ty::Str)
                {
                    let start = self.place(*value).project(Projection::Field(0));
                    return Rvalue::Cast {
                        operand: Operand::Copy(start),
                        ty,
                    };
                }
                let operand = self.number(*value);
                // An enum with numbers written for its variants, `W = 87`: its tag is which
                // variant it is, and picks the number from a table of them.
                if let Ty::Struct(def, _) = *self.l.mir.types.get(from)
                    && let DefKind::Struct(s) = self.l.names.def(def).kind
                    && self.l.ast.structs[s].has_values()
                {
                    return self.variant_number(s, operand, ty, self.span(expr));
                }
                if from == ty {
                    Rvalue::Use(operand)
                } else {
                    Rvalue::Cast { operand, ty }
                }
            }
            Expr::Range {
                start: Some(start),
                end: Some(end),
            } => {
                let ty = self.expr_ty(expr);
                let (start, end) = (self.operand(*start), self.operand(*end));
                Rvalue::Range { ty, start, end }
            }
            Expr::ArrayRepeat { value, .. } => {
                let operand = self.operand(*value);
                let len = self.typed.count(expr);
                let len = self.value_of(len);
                self.collection(expr, len, |ty| Rvalue::Repeat { ty, operand })
            }
            // `p.resize(n)`: `p = realloc(p, …)`.
            Expr::MethodCall { receiver, args, .. }
                if self.typed.pointer_method(expr) == Some(PointerMethod::Resize) =>
            {
                let place = self.place(*receiver);
                let ty = self.place_ty(&place);
                let count = self.operand(args[0]);
                let resized = Rvalue::Intrinsic {
                    intrinsic: Intrinsic::Resize,
                    args: vec![Operand::Copy(place.clone()), count],
                    ty,
                };
                self.assign(place, resized, self.span(expr));
                let unit = self.l.mir.types.unit;
                Rvalue::Use(self.constant(ConstKind::Unit, unit))
            }
            Expr::MethodCall { receiver, args, .. }
                if let Some(method) = self.typed.pointer_method(expr) =>
            {
                let intrinsic = match method {
                    PointerMethod::Resize => Intrinsic::Resize,
                    PointerMethod::Free => Intrinsic::Free,
                    PointerMethod::Read => Intrinsic::Read,
                    PointerMethod::Write => Intrinsic::Write,
                    PointerMethod::At | PointerMethod::AtMut => Intrinsic::Offset,
                    PointerMethod::View | PointerMethod::ViewMut => Intrinsic::View,
                    PointerMethod::IsNull => Intrinsic::IsNull,
                };
                let args = std::iter::once(*receiver)
                    .chain(args.iter().copied())
                    .map(|a| self.operand(a))
                    .collect();
                let ty = self.expr_ty(expr);
                Rvalue::Intrinsic {
                    intrinsic,
                    args,
                    ty,
                }
            }
            Expr::MethodCall { receiver, args, .. }
                if let Some(method) = self.typed.array_method(expr) =>
            {
                Rvalue::Use(self.array_method(expr, method, *receiver, args))
            }
            Expr::MethodCall { receiver, args, .. } => {
                let def = self
                    .typed
                    .method(expr)
                    .expect("checked method calls are resolved");
                let DefKind::Func(func) = self.l.names.def(def).kind else {
                    unreachable!("methods are functions")
                };
                self.call(expr, func, Some(*receiver), args)
            }
            // `items.len` of an array, slice or text.
            Expr::Field { object, .. } if let Some(method) = self.typed.array_method(expr) => {
                Rvalue::Use(self.array_method(expr, method, *object, &[]))
            }
            // `list.len`: property `len`, called with `list` as its `self`.
            Expr::Field { object, .. } if let Some(def) = self.typed.method(expr) => {
                let DefKind::Func(func) = self.l.names.def(def).kind else {
                    unreachable!("properties are functions")
                };
                self.call(expr, func, Some(*object), &[])
            }
            _ => Rvalue::Use(self.operand_unwrapped(expr)),
        }
    }

    /// Whether `expr` reads a property, `list.len`, which is a call, rather than a field;
    /// or an array's length, `items.len`.
    pub(super) fn is_prop(&self, expr: Id<Expr>) -> bool {
        match self.l.ast.exprs[expr] {
            Expr::Field { .. } => {
                self.typed.method(expr).is_some() || self.typed.array_method(expr).is_some()
            }
            // `values[i]` of a slice or struct is a place: what its `at` lends.
            Expr::Index { .. } => false,
            _ => false,
        }
    }

    /// The number `tag`, which variant of enum `s` a value is, converts to as a `ty`: the
    /// one written for that variant, read from a table of them all.
    pub(super) fn variant_number(
        &mut self,
        s: Id<Struct>,
        tag: Operand,
        ty: TypeId,
        span: Span,
    ) -> Rvalue {
        let strukt = &self.l.ast.structs[s];
        let items: Vec<Operand> = (0..strukt.variants().len())
            .map(|i| strukt.variant_value(i))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|n| self.constant(ConstKind::Int(n), ty))
            .collect();
        let len = Ty::Value(Value::Int(items.len() as u128));
        let len = self.l.mir.types.intern(len);
        let table_ty = self.l.mir.types.intern(Ty::Array { element: ty, len });
        let table = self.temp(table_ty, span.clone());
        self.assign(
            Place::local(table),
            Rvalue::Array {
                ty: table_ty,
                items,
            },
            span.clone(),
        );
        let usize = self.l.mir.types.intern(Ty::Int(IntTy::USize));
        let index = self.temp(usize, span.clone());
        self.assign(
            Place::local(index),
            Rvalue::Cast {
                operand: tag,
                ty: usize,
            },
            span,
        );
        let element = Place::local(table).project(Projection::Index(index));
        Rvalue::Use(Operand::Copy(element))
    }

    /// A collection literal of `len` elements, made by `make` given its array type. Where a
    /// slice is expected, the array is kept in storage of its own until the end of the
    /// enclosing block, and the slice views it.
    pub(super) fn collection(
        &mut self,
        expr: Id<Expr>,
        len: TypeId,
        make: impl FnOnce(TypeId) -> Rvalue,
    ) -> Rvalue {
        let ty = self.expr_ty(expr);
        let types = &mut self.l.mir.types;
        let Ty::Ref { mutable, to } = *types.get(ty) else {
            return make(ty);
        };
        let Ty::Slice(element) = *types.get(to) else {
            unreachable!("checked: collection literals are arrays or slices")
        };
        let array_ty = types.intern(Ty::Array { element, len });
        let reference_ty = types.intern(Ty::Ref {
            mutable,
            to: array_ty,
        });
        let span = self.span(expr);
        let storage = self.new_local(array_ty, None, mutable, span.clone());
        self.scopes
            .last_mut()
            .expect("expressions are inside a scope")
            .push(storage);
        self.push(StatementKind::StorageLive(storage), span.clone());
        self.assign(Place::local(storage), make(array_ty), span.clone());
        let reference = self.temp(reference_ty, span.clone());
        let borrow = Rvalue::Ref {
            mutable,
            place: Place::local(storage),
        };
        self.assign(Place::local(reference), borrow, span);
        Rvalue::Cast {
            operand: Operand::Move(Place::local(reference)),
            ty,
        }
    }
}
