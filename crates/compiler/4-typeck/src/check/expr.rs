use std::collections::HashSet;

use som_ast::{BinOp, Expr, Id, Stmt, VariantKind};
use som_resolve::DefKind;

use super::Checker;
use super::matching::holds;
use super::narrow::Narrowing;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

impl Checker<'_> {
    pub(super) fn infer(&mut self, id: Id<Expr>) -> TypeId {
        let in_place = std::mem::replace(&mut self.hints.in_place, false);
        self.hints.expected_here = self.hints.expected.take();
        let mut ty = self.infer_inner(id, in_place);
        // `I::Item` of an `I` the expression's arguments decided: what its implementation says.
        ty = self.normalize_assoc(ty);
        if let Some(inner) = self.narrowed_type(id) {
            self.records.narrowed_reads.insert(id);
            ty = inner;
        }
        self.exprs[id.id] = Some(ty);
        ty
    }

    /// For `==` with an optional on either side, the types whose values are compared: what each
    /// optional holds. `None` if neither side is optional.
    pub(super) fn optional_operands(&self, lhs: TypeId, rhs: TypeId) -> Option<(TypeId, TypeId)> {
        let inner = |ty: TypeId| match *self.u.shallow_ty(ty) {
            Ty::Optional(inner) => Some(inner),
            _ => None,
        };
        match (inner(lhs), inner(rhs)) {
            (None, None) => None,
            (l, r) => Some((l.unwrap_or(lhs), r.unwrap_or(rhs))),
        }
    }

    /// Infers `exprs` for what they say about the types in them: they're arguments to a call
    /// that's wrong.
    pub(super) fn infer_all(&mut self, exprs: &[Id<Expr>]) {
        for &expr in exprs {
            self.infer(expr);
        }
    }

    /// Infers `expr` as the optional it is, even where it's narrowed: for `if x`, `x != none`
    /// and `x else …`, which ask about the optional itself.
    pub(super) fn infer_optional(&mut self, expr: Id<Expr>) -> TypeId {
        let Some(key) = self.key(expr) else {
            return self.infer(expr);
        };
        let (lifted, kept): (Vec<Narrowing>, Vec<Narrowing>) = std::mem::take(&mut self.narrowed)
            .into_iter()
            .partition(|n| n.key.same(&key));
        self.narrowed = kept;
        let ty = self.infer(expr);
        self.narrowed.extend(lifted);
        ty
    }

    /// Infers `object`, whose place is part of a larger one: the `p` in `p.x` or `*p`.
    pub(super) fn infer_object(&mut self, object: Id<Expr>) -> TypeId {
        self.hints.in_place = true;
        self.infer(object)
    }

    /// `n` in `n.len` of a `&&string`: a place reached through references to references is
    /// read through all but the last, as operators read through all of them. Gives the type
    /// it's then read as, `&string`.
    pub(super) fn through_references(&mut self, object: Id<Expr>, ty: TypeId) -> TypeId {
        let is_place = matches!(
            self.ast.exprs[object],
            Expr::Var(_) | Expr::Field { .. } | Expr::Deref(_) | Expr::Index { .. }
        );
        let (mut ty, mut depth) = (self.u.shallow(ty), 0);
        while let Ty::Ref { to, .. } = *self.u.get(ty)
            && matches!(self.u.shallow_ty(to), Ty::Ref { .. })
            && is_place
        {
            (ty, depth) = (self.u.shallow(to), depth + 1);
        }
        if depth > 0 {
            self.records.derefs.insert(object, depth);
            self.exprs[object.id] = Some(ty);
        }
        ty
    }

    fn infer_inner(&mut self, id: Id<Expr>, in_place: bool) -> TypeId {
        let table = &self.u.table;
        let (error, bool) = (table.error, table.bool);

        match &self.ast.exprs[id] {
            Expr::Hole => error,
            Expr::Type(_) => {
                self.errors.push(TypeError::TypeNotCompared {
                    span: self.span(id),
                });
                error
            }
            // `@if T == f32`: a question about types, answered while compiling.
            Expr::Binary {
                op: BinOp::Equals | BinOp::NotEquals,
                lhs,
                rhs,
            } if self.is_type_operand(*lhs) || self.is_type_operand(*rhs) => {
                self.infer_type_comparison(*lhs, *rhs)
            }
            // `@const f(x)`: it names no locals, so it's checked here like anything else.
            Expr::InPlace(c) => {
                self.checked_consts.insert(*c);
                self.hints.expected = self.hints.expected_here.take();
                self.infer(self.ast.consts[*c].value)
            }
            // Never gets a value: it's whatever's expected here, or if nothing decides,
            // `()` once checking is done.
            Expr::Unreachable => {
                let ty =
                    (self.hints.expected_here.take()).unwrap_or_else(|| self.u.fresh(VarKind::Any));
                self.deferred.unreachables.push(ty);
                ty
            }
            // `@fail("…")`: like `@unreachable`; its message is text.
            Expr::Fail(message) => {
                let ty =
                    (self.hints.expected_here.take()).unwrap_or_else(|| self.u.fresh(VarKind::Any));
                let text = self.u.table.text;
                self.expect(*message, text, None);
                self.deferred.unreachables.push(ty);
                ty
            }
            Expr::Int(_) => self.u.fresh(VarKind::Int),
            Expr::Float(_) => self.u.fresh(VarKind::Float),
            Expr::Bool(_) => bool,
            // A `str`, or a `cstr` where C needs one.
            Expr::Str(_) => self.u.fresh(VarKind::Text),
            Expr::Interpolated(parts) => self.infer_interpolated(parts),
            Expr::None => {
                let inner = self.u.fresh(VarKind::Any);
                self.u.intern(Ty::Optional(inner))
            }
            // A collection: an array, or a slice where one is expected.
            Expr::Array(items) => self.infer_array(items),
            Expr::Cast { value, ty } => self.infer_cast(id, *value, *ty),
            Expr::Range { start, end } => {
                let int = self.u.fresh(VarKind::Int);
                for &bound in start.iter().chain(end) {
                    self.expect(bound, int, None);
                }
                // Only `slice` knows where an open range ends; it checks those itself.
                if start.is_none() || end.is_none() {
                    self.errors.push(TypeError::OpenRange {
                        span: self.span(id),
                    });
                }
                self.u.intern(Ty::Range(int))
            }
            Expr::ArrayRepeat { value, count } => {
                let element = self.infer(*value);
                if !self.is_copy(element) {
                    self.errors.push(TypeError::NotCopyable {
                        ty: self.describe(element),
                        span: self.span(*value),
                        repeat: true,
                    });
                }
                let len = self.lower_value(*count, None);
                self.records.counts.insert(id, len);
                self.u.fresh(VarKind::Collection { element, len })
            }
            Expr::Fallback { optional, fallback } => {
                // What's expected of the whole, `&mut Pair`, is expected of the optional
                // wrapped: so `get(i) else …` picks the `get` giving a `&mut Pair?`.
                if let Some(expected) = self.hints.expected_here.take() {
                    self.hints.expected = Some(self.u.intern(Ty::Optional(expected)));
                }
                let ty = self.infer_optional(*optional);
                let inner = self.optional_inner(ty, *optional);
                let from = self.span(*optional);
                self.expect(*fallback, inner, Some(from));
                inner
            }

            Expr::Var(path) if let Some((primitive, _)) = path.int_limit() => {
                self.u.intern(Ty::from_primitive(primitive))
            }
            Expr::Var(_) => self.infer_var(id, in_place),

            Expr::Binary {
                op: BinOp::And | BinOp::Or,
                ..
            } => {
                self.check_condition(id);
                bool
            }

            Expr::Binary { op, lhs, rhs } => self.infer_binary(id, *op, *lhs, *rhs),

            // `!x` of an optional is `x == none`.
            Expr::Not(_) => {
                self.check_condition(id);
                bool
            }

            Expr::BitNot(inner) => self.infer_bit_not(id, *inner),

            Expr::Neg(inner) => self.infer_neg(id, *inner),

            Expr::Borrow { mutable, expr } => self.infer_borrow(*mutable, *expr),

            // The parameter it's passed to says what the variable is.
            Expr::Out(_) => {
                let to = self.u.fresh(VarKind::Any);
                if let Some(def) = self.names.expr(id) {
                    self.locals.insert(def, to);
                }
                self.u.intern(Ty::Ref { mutable: true, to })
            }

            Expr::Deref(inner) => self.infer_deref(id, *inner),

            Expr::Assign { target, value } => self.infer_assign(*target, *value),

            Expr::Call {
                callee,
                owner_args,
                type_args: written,
                args,
            } => self.infer_call(id, callee, owner_args, written, args),

            Expr::Closure { params, body } => self.infer_closure(id, params, *body),

            Expr::CallValue { callee, args } => {
                let ty = self.infer(*callee);
                let name = self.span(*callee).get_text().to_string();
                self.call_value(id, ty, &name, args)
            }

            Expr::MethodCall {
                receiver,
                method,
                args,
            } => self.infer_method_call(id, *receiver, method, args),

            Expr::Field { object, field } => self.infer_field(id, *object, field),

            Expr::Index { object, index } => self.index(id, *object, *index),

            Expr::Construct {
                ty: path,
                args: written,
                fields: given,
            } => self.infer_construct(id, path, written, given),

            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => self.infer_conditional(*condition, *then, *otherwise),

            // `@for (i, k) in T` among its arms, over a generic enum: checked for each instance.
            Expr::Match { scrutinee, .. }
                if let Some(per_type) = self.ast.per_type_matches.get(&id) =>
            {
                self.infer_per_type_match(id, *scrutinee, per_type)
            }
            Expr::Match { scrutinee, arms } => {
                let discarded = std::mem::take(&mut self.hints.discarding);
                self.check_match(id, *scrutinee, arms, discarded)
            }

            Expr::Block { stmts, value } => self.infer_block(stmts, *value),
        }
    }

    fn infer_var(&mut self, id: Id<Expr>, in_place: bool) -> TypeId {
        let error = self.u.table.error;
        match self
            .names
            .expr(id)
            .map(|def| (def, &self.names.def(def).kind))
        {
            // An `out` parameter is a pointer, but it's used as what it points to.
            Some((def, &DefKind::Param { func, index }))
                if self.ast.funcs[func].params[index].out =>
            {
                let ty = self.locals.get(&def).copied().unwrap_or(error);
                match self.u.get(ty) {
                    Ty::Ref { to, .. } => *to,
                    _ => error,
                }
            }
            Some((
                def,
                DefKind::Param { .. }
                | DefKind::Local(_)
                | DefKind::Out(_)
                | DefKind::Binding(_)
                | DefKind::ClosureParam { .. },
            )) => {
                let ty = self.locals.get(&def).copied().unwrap_or(error);
                // Passed on, a `&mut` can change what's behind it.
                if !in_place && self.is_mut_ref(ty) {
                    self.forget(|key| key.root == def && !key.path.is_empty());
                }
                ty
            }
            Some((def, DefKind::Variant { .. })) => {
                let (ty, s, index, _) = self.variant_value(def);
                self.fit_alias(id, ty);
                let kind = self.ast.structs[s].variants()[index].kind;
                if kind != VariantKind::Unit {
                    self.errors.push(TypeError::WrongVariantForm {
                        name: self.variant_name(s, index),
                        holds: holds(kind),
                        form: self.variant_form(s, index),
                        span: self.span(id),
                    });
                }
                ty
            }
            Some((_, &DefKind::Const(c))) => self.check_const(c),
            // `N` in `struct Buffer<N: usize>`: the number it is in each copy.
            Some((def, _)) if let Some(&ty) = self.value_params.get(&def) => ty,
            Some((def, DefKind::Func(func))) => {
                let expected = self.hints.expected_here.take();
                let func = self.pick_function(id, def, expected).unwrap_or(*func);
                let (_, args) = self.instantiate(func);
                self.require_bounds(func, &args, self.span(id));
                if !args.is_empty() {
                    self.type_args.insert(id, args.clone());
                }
                self.u.intern(Ty::FnItem(self.names.func(func), args))
            }
            _ => error,
        }
    }

    fn infer_interpolated(&mut self, parts: &[som_ast::Part]) -> TypeId {
        for &part in parts {
            match part {
                som_ast::Part::Text(text) => {
                    let ty = self.infer(text);
                    self.text_as_string(ty);
                }
                som_ast::Part::Value(value) => {
                    let ty = self.infer(value);
                    self.text_as_string(ty);
                    self.deferred.displayed.push((value, ty));
                }
            }
        }
        self.u.table.text
    }

    fn infer_borrow(&mut self, mutable: bool, expr: Id<Expr>) -> TypeId {
        // `&mut p` can change anything inside `p`, other than through what's narrowed:
        // `&mut x` of a narrowed `x` borrows the value it holds.
        if mutable && let Some(borrowed) = self.key(expr) {
            self.forget(|key| key.within(&borrowed) && !key.same(&borrowed));
        }
        let to = self.infer(expr);
        if mutable {
            self.check_mutable(expr, true);
        }
        self.u.intern(Ty::Ref {
            mutable: mutable,
            to,
        })
    }

    fn infer_deref(&mut self, id: Id<Expr>, inner: Id<Expr>) -> TypeId {
        let error = self.u.table.error;
        let ty = self.infer_object(inner);
        let ty = self.u.shallow(ty);
        match self.u.get(ty).clone() {
            Ty::Ref { to, .. } if matches!(self.u.shallow_ty(to), Ty::Slice(_) | Ty::Str) => {
                self.errors.push(TypeError::DerefSlice {
                    span: self.span(id),
                });
                error
            }
            Ty::Ref { to, .. } => to,
            Ty::Error => error,
            Ty::Var(_) if self.u.var_kind(ty) == Some(VarKind::Any) => {
                self.errors.push(TypeError::CannotInfer {
                    span: self.span(inner),
                });
                error
            }
            _ => {
                self.errors.push(TypeError::CannotDeref {
                    ty: self.describe(ty),
                    span: self.span(inner),
                });
                error
            }
        }
    }

    fn infer_assign(&mut self, target: Id<Expr>, value: Id<Expr>) -> TypeId {
        let unit = self.u.table.unit;
        // The target is the optional itself, not the value it held: `x = none` is fine.
        let assigned = self.key(target);
        let (lifted, kept) = std::mem::take(&mut self.narrowed)
            .into_iter()
            .partition(|n| assigned.as_ref().is_some_and(|a| n.key.same(a)));
        self.narrowed = kept;
        let outer = std::mem::replace(&mut self.hints.assigning, true);
        let target_ty = self.infer(target);
        self.hints.assigning = outer;
        // After inferring it: whether `p.x` is a field or a property depends on `p`.
        if let Expr::Field { object, field } = &self.ast.exprs[target]
            && self.is_prop(target)
        {
            let object_ty = self.exprs[object.id].unwrap_or(self.u.table.error);
            self.errors.push(TypeError::AssignToProp {
                name: field.name.to_string(),
                span: self.span(target),
                field: self.hidden_field_named(object_ty, field),
            });
        } else if !self.is_place(target) {
            self.errors.push(TypeError::InvalidAssignTarget {
                span: self.span(target),
            });
        }
        self.check_mutable(target, false);
        // The value is worked out first, while the target still holds what it did:
        // `x = x + 1` reads the narrowed `x`.
        self.narrowed.extend::<Vec<Narrowing>>(lifted);
        let from = self.span(target);
        self.expect(value, target_ty, Some(from));
        if let Some(assigned) = &assigned {
            self.forget(|key| key.within(assigned));
        }
        self.narrow_if_assigned_a_value(assigned, target_ty, value);
        unit
    }

    fn infer_conditional(
        &mut self,
        condition: Id<Expr>,
        then: Id<Expr>,
        otherwise: Id<Expr>,
    ) -> TypeId {
        // Like an `if` statement's: an optional is narrowed in the branch it holds in.
        let facts = self.check_condition(condition);
        let (then_ty, _) = self.narrowed_during(facts.when_true, |c| c.infer(then));
        let (otherwise_ty, _) = self.narrowed_during(facts.when_false, |c| c.infer(otherwise));
        // A branch that never finishes (`return`) has no value: the other's is it.
        let (then_leaves, otherwise_leaves) = (self.expr_leaves(then), self.expr_leaves(otherwise));
        for (branch, leaves) in [(then, then_leaves), (otherwise, otherwise_leaves)] {
            if leaves {
                self.records.diverging.insert(branch);
            }
        }
        match (then_leaves, otherwise_leaves) {
            (true, true) => return self.u.fresh(VarKind::Any),
            (true, false) => return otherwise_ty,
            (false, true) => return then_ty,
            (false, false) => {}
        }
        // `x if c else none`: either branch can be the optional the other is wrapped in.
        if self.coerce(otherwise, then_ty, otherwise_ty) {
            then_ty
        } else if self.coerce(then, otherwise_ty, then_ty) {
            otherwise_ty
        } else {
            let from = self.span(then);
            self.mismatch(then_ty, otherwise_ty, self.span(otherwise), Some(from));
            then_ty
        }
    }

    fn infer_per_type_match(
        &mut self,
        id: Id<Expr>,
        scrutinee: Id<Expr>,
        per_type: &som_ast::PerType,
    ) -> TypeId {
        let error = self.u.table.error;
        let name = &per_type.of.name().name;
        let param = (self.current_func).and_then(|f| {
            (self.names.type_params(f).iter().copied())
                .find(|&p| &*self.names.def(p).name == &**name)
        });
        let Some(param) = param else {
            self.errors.push(TypeError::ReflectNotEnum {
                name: name.to_string(),
                span: per_type.of.span(),
            });
            return error;
        };
        let Some(&ty) = self.bound.get(&param) else {
            // What it gives is what it's expected to: it's checked to be, per instance.
            self.infer(scrutinee);
            self.records.match_regions.insert(id);
            if let Some(func) = self.current_func {
                self.records.per_instance.insert(func);
            }
            return match self.hints.expected_here.take() {
                Some(expected) => expected,
                None => self.u.fresh(VarKind::Any),
            };
        };
        let enum_ = match self.u.get(ty) {
            Ty::Struct(def, _) => match self.names.def(*def).kind {
                DefKind::Struct(s) if self.ast.structs[s].is_enum() => Some(s),
                _ => None,
            },
            _ => None,
        };
        match enum_.and_then(|s| per_type.arms.iter().find(|(a, _)| *a == s)) {
            Some(&(_, copy)) => {
                self.records.chosen_matches.insert(id, copy);
                self.infer(copy)
            }
            None => {
                self.errors.push(TypeError::ReflectNotEnum {
                    name: self.describe(ty).plain().trim_matches('`').to_string(),
                    span: per_type.of.span(),
                });
                error
            }
        }
    }

    fn infer_block(&mut self, stmts: &[Id<Stmt>], value: Option<Id<Expr>>) -> TypeId {
        let unit = self.u.table.unit;
        // What the block is expected to be is what its final value is.
        let expected = self.hints.expected_here.take();
        // Like `check_stmts`, but what's narrowed in here lasts into the final value.
        let outer: HashSet<usize> = self.narrowed.iter().map(|n| n.id).collect();
        for &stmt in stmts {
            self.check_stmt(stmt);
        }
        let ty = match value {
            Some(value) => {
                self.hints.expected = expected;
                let ty = self.infer(value);
                if matches!(
                    self.ast.exprs[value],
                    Expr::Call { .. } | Expr::MethodCall { .. }
                ) {
                    self.deferred.tail_calls.push((value, ty));
                }
                ty
            }
            // `{ return x; }` never ends with a value: like `@unreachable`, it fits
            // whatever's expected, as in `get(i) else { return none; }`.
            None if self.leaves(stmts) => {
                let ty = expected.unwrap_or_else(|| self.u.fresh(VarKind::Any));
                self.deferred.unreachables.push(ty);
                ty
            }
            None => unit,
        };
        self.narrowed.retain(|n| outer.contains(&n.id));
        ty
    }
}
