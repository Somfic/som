use som_ast::{BinOp, Expr, Func, Id, Span};
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::error::TypeError;
use crate::ty::{IntTy, Ty, TypeId, Value};
use crate::unify::VarKind;

impl Checker<'_> {
    /// Whether member `func`'s `@if`s hold for `receiver`, the type it's used on: its impl
    /// block's parameters stand for what's in their places in `receiver`. `None` where that
    /// isn't known, a generic type's.
    pub(super) fn member_holds(&mut self, func: Id<Func>, receiver: TypeId) -> Option<bool> {
        let when = self.ast.funcs[func].when.clone();
        if when.is_empty() {
            return Some(true);
        }
        let saved = self.bound.clone();
        if let Some(index) = self.names.impl_of(func)
            && let Some(target) = self.impls[index].target
        {
            let params = self.impls[index].params.clone();
            let mut found = self.u.shallow(receiver);
            if let Ty::Ref { to, .. } = *self.u.get(found) {
                found = self.u.shallow(to);
            }
            let mut matched = Vec::new();
            self.match_params(target, found, &params, &mut matched);
            // The conditions are in terms of the block's parameters.
            for (param, arg) in matched {
                let arg = match *self.u.get(arg) {
                    Ty::Param(p) => self.bound.get(&p).copied().unwrap_or(arg),
                    _ => arg,
                };
                if !matches!(self.u.get(arg), Ty::Var(_) | Ty::Error) {
                    self.bound.insert(param, arg);
                }
            }
        }
        let mut holds = Some(true);
        for condition in when {
            match self.settled_bool(condition) {
                Some(true) => {}
                Some(false) => {
                    holds = Some(false);
                    break;
                }
                None => holds = None,
            }
        }
        self.bound = saved;
        holds
    }

    /// How a member's condition reads: as written, or for an `else`, `!(N >= 3)`.
    fn condition_text(&self, condition: Id<Expr>) -> String {
        match self.ast.exprs[condition] {
            Expr::Not(inner) if self.span(inner) == self.span(condition) => {
                format!("!({})", self.span(inner).get_text())
            }
            _ => self.span(condition).get_text().to_string(),
        }
    }

    /// What's in `found` where each of `params` is in `pattern`, an impl block's target.
    fn match_params(
        &self,
        pattern: TypeId,
        found: TypeId,
        params: &[DefId],
        matched: &mut Vec<(DefId, TypeId)>,
    ) {
        let found = self.u.shallow(found);
        let pattern_ty = self.u.get(pattern).clone();
        if let Ty::Param(p) = pattern_ty
            && params.contains(&p)
        {
            matched.push((p, found));
            return;
        }
        if let Some(pairs) = pattern_ty.zip_children(self.u.get(found)) {
            for (a, b) in pairs {
                self.match_params(a, b, params, matched);
            }
        }
    }

    /// Reports member `func`, used as `name` at `span` on a value of type `ty`, if its `@if`s
    /// don't hold there, or aren't known to. Whether it's there.
    pub(super) fn check_member_there(
        &mut self,
        func: Id<Func>,
        ty: TypeId,
        name: &str,
        span: Span,
    ) -> bool {
        let holds = self.member_holds(func, ty);
        if holds == Some(true) {
            return true;
        }
        let condition = (self.ast.funcs[func].when.iter())
            .map(|&c| self.condition_text(c))
            .collect::<Vec<_>>()
            .join(" && ");
        let error = match holds {
            Some(false) => TypeError::MemberNotHere {
                name: name.to_string(),
                ty: self.describe(ty),
                condition,
                span,
            },
            _ => TypeError::MemberMaybeNotHere {
                name: name.to_string(),
                condition,
                span,
            },
        };
        self.errors.push(error);
        false
    }

    /// Whether `expr` is a number known while compiling: a literal, a constant made of them,
    /// or a function's value parameter, `N`, known in each instance.
    fn settled_int(&self, expr: Id<Expr>) -> bool {
        if self.number(expr).is_some() {
            return true;
        }
        match &self.ast.exprs[expr] {
            Expr::Var(_) => self.names.expr(expr).is_some_and(|def| {
                matches!(
                    self.names.def(def).kind,
                    DefKind::TypeParam { value: true, .. }
                        | DefKind::ImplParam { value: true, .. }
                        | DefKind::StructParam { value: true, .. }
                )
            }),
            Expr::Binary { lhs, rhs, .. } => self.settled_int(*lhs) && self.settled_int(*rhs),
            Expr::Cast { value, .. } | Expr::Neg(value) => self.settled_int(*value),
            _ => false,
        }
    }

    /// Whether what an `@for` loops over is known while compiling: a range of known numbers,
    /// or an array written out, each of its elements a copy of the body.
    pub(super) fn settled_iterable(&self, expr: Id<Expr>) -> bool {
        match &self.ast.exprs[expr] {
            Expr::Range {
                start: Some(start),
                end: Some(end),
            } => self.settled_int(*start) && self.settled_int(*end),
            Expr::Array(_) => true,
            _ => false,
        }
    }

    /// The value of `@if`'s condition `expr`, if it's known while compiling: a `bool`, a
    /// constant that's one, comparisons of known numbers, and `!`, `&&` and `||` of those.
    pub(super) fn settled_bool(&mut self, expr: Id<Expr>) -> Option<bool> {
        match &self.ast.exprs[expr] {
            // `T == f32`: known once `T` is.
            Expr::Binary {
                op: op @ (BinOp::Equals | BinOp::NotEquals),
                lhs,
                rhs,
            } if self.is_type_operand(*lhs) || self.is_type_operand(*rhs) => {
                let (a, b) = (self.type_operand(*lhs)?, self.type_operand(*rhs)?);
                let same = self.same_type(a, b)?;
                Some(same == (*op == BinOp::Equals))
            }
            Expr::Bool(b) => Some(*b),
            Expr::Not(inner) => self.settled_bool(*inner).map(|b| !b),
            Expr::Var(_) if let Some(Value::Bool(b)) = self.bound_value(expr) => Some(b),
            Expr::Var(_) => match self.names.def(self.names.expr(expr)?).kind {
                DefKind::Const(c) => match self.known.get(c) {
                    Some(crate::ConstValue::Bool(b)) => Some(b),
                    Some(_) => None,
                    None => self.settled_bool(self.ast.consts[c].value),
                },
                _ => None,
            },
            Expr::Binary { op, lhs, rhs } => match op {
                BinOp::And => Some(self.settled_bool(*lhs)? && self.settled_bool(*rhs)?),
                BinOp::Or => Some(self.settled_bool(*lhs)? || self.settled_bool(*rhs)?),
                _ => {
                    if let (Some(a), Some(b)) = (self.settled_bool(*lhs), self.settled_bool(*rhs)) {
                        return match op {
                            BinOp::Equals => Some(a == b),
                            BinOp::NotEquals => Some(a != b),
                            _ => None,
                        };
                    }
                    crate::consts::compare(*op, self.number(*lhs)?, self.number(*rhs)?)
                }
            },
            _ => None,
        }
    }

    /// The number `n`, written where a type argument goes, of type `expected` or of any
    /// integer type for an array's length.
    pub(super) fn lower_number(&mut self, n: u128, expected: Option<TypeId>, span: Span) -> TypeId {
        let error = self.u.table.error;
        let int = expected.unwrap_or(self.u.table.usize);
        match self.u.get(int).clone() {
            Ty::Int(int_ty) => {
                let max: u128 = match int_ty {
                    IntTy::I8 => i8::MAX as u128,
                    IntTy::I16 => i16::MAX as u128,
                    IntTy::I32 => i32::MAX as u128,
                    IntTy::I64 | IntTy::ISize => i64::MAX as u128,
                    IntTy::I128 => i128::MAX as u128,
                    IntTy::U8 => u8::MAX as u128,
                    IntTy::U16 => u16::MAX as u128,
                    IntTy::U32 => u32::MAX as u128,
                    IntTy::U64 | IntTy::USize => u64::MAX as u128,
                    IntTy::U128 => u128::MAX,
                };
                if n > max {
                    self.errors.push(TypeError::LiteralOutOfRange {
                        value: n.min(i128::MAX as u128) as i128,
                        ty: int_ty.name().to_string(),
                        max,
                        min: 0,
                        span,
                        type_arg: true,
                    });
                    return error;
                }
                return self.u.intern(Ty::Value(Value::Int(n)));
            }
            _ => {
                let found = self.u.fresh(VarKind::Int);
                if self.u.unify(int, found).is_err() {
                    self.mismatch(int, found, span, None);
                    return error;
                }
                self.u.intern(Ty::Value(Value::Int(n)))
            }
        }
    }

    pub(super) fn const_number(&self, expr: Id<Expr>) -> Option<i128> {
        crate::const_number_with(self.ast, self.names, &self.known, expr)
    }

    /// The value a generic value parameter stands for, checking one instance: `N` is `3`.
    fn bound_value(&self, expr: Id<Expr>) -> Option<Value> {
        let def = self.names.expr(expr)?;
        match self.u.table.get(*self.bound.get(&def)?) {
            Ty::Value(value) => Some(*value),
            _ => None,
        }
    }

    /// The number `expr` is while compiling: `const_number`, and a value parameter bound in
    /// an instance (`N + 1`).
    fn number(&self, expr: Id<Expr>) -> Option<i128> {
        if let Some(n) = self.const_number(expr) {
            return Some(n);
        }
        Some(match &self.ast.exprs[expr] {
            Expr::Var(_) => match self.bound_value(expr)? {
                Value::Int(n) => n as i128,
                Value::Bool(_) => return None,
            },
            Expr::Neg(inner) => self.number(*inner)?.checked_neg()?,
            Expr::Cast { value, .. } => self.number(*value)?,
            Expr::Binary { op, lhs, rhs } => {
                crate::consts::fold(*op, self.number(*lhs)?, self.number(*rhs)?)?
            }
            _ => return None,
        })
    }

    /// Whether `expr` names a type, compared with another in an `@if`: `f32`, `T`, `Point`.
    pub(super) fn is_type_operand(&self, expr: Id<Expr>) -> bool {
        match self.ast.exprs[expr] {
            Expr::Type(_) => true,
            Expr::Var(_) => self.names.expr(expr).is_some_and(|def| {
                matches!(
                    self.names.def(def).kind,
                    DefKind::TypeParam { value: false, .. }
                        | DefKind::ImplParam { value: false, .. }
                        | DefKind::StructParam { value: false, .. }
                        | DefKind::Struct(_)
                )
            }),
            _ => false,
        }
    }

    /// The type `expr` names, if it names one: a type parameter is what it's bound to, checking
    /// an instance.
    pub(super) fn type_operand(&mut self, expr: Id<Expr>) -> Option<TypeId> {
        if !self.is_type_operand(expr) {
            return None;
        }
        Some(match self.ast.exprs[expr] {
            Expr::Type(ty) => self.lower(ty),
            _ => {
                let def = self.names.expr(expr)?;
                match self.names.def(def).kind {
                    DefKind::Struct(_) => self.u.intern(Ty::Struct(def, Vec::new())),
                    _ => match self.bound.get(&def) {
                        Some(&bound) => bound,
                        None => self.u.intern(Ty::Param(def)),
                    },
                }
            }
        })
    }

    /// Whether `a` and `b` are the same type, if that's known: not while either has a type
    /// parameter in it, or something not decided yet. A generic struct named without its
    /// arguments, `Pair`, is any `Pair`.
    fn same_type(&mut self, a: TypeId, b: TypeId) -> Option<bool> {
        let (a, b) = (self.u.normalize(a), self.u.normalize(b));
        if !self.settled_type(a) || !self.settled_type(b) {
            return None;
        }
        Some(match (self.u.get(a), self.u.get(b)) {
            (Ty::Struct(x, xs), Ty::Struct(y, ys)) if xs.is_empty() || ys.is_empty() => x == y,
            _ => a == b,
        })
    }

    /// Whether `ty` is known as it is: no type parameter or undecided type in it.
    fn settled_type(&self, ty: TypeId) -> bool {
        let ty = self.u.shallow(ty);
        match self.u.get(ty) {
            Ty::Var(_) | Ty::Param(_) | Ty::Error => false,
            t => t.children().into_iter().all(|c| self.settled_type(c)),
        }
    }

    /// Whether `expr` is worked out from the function's generic value parameters (and
    /// literals and constants): `N >= 3`. An `@if` on it is checked for each instance.
    pub(super) fn over_params(&self, expr: Id<Expr>) -> bool {
        fn walk(c: &Checker, expr: Id<Expr>, found: &mut bool) -> bool {
            match &c.ast.exprs[expr] {
                Expr::Int(_) | Expr::Bool(_) | Expr::Type(_) => true,
                Expr::Var(_) => match c.names.expr(expr).map(|def| &c.names.def(def).kind) {
                    // `N`, or `T` of `T == f32`.
                    Some(DefKind::TypeParam { .. } | DefKind::ImplParam { .. }) => {
                        *found = true;
                        true
                    }
                    Some(DefKind::Const(_) | DefKind::Struct(_)) => true,
                    _ => false,
                },
                Expr::Not(inner) | Expr::Neg(inner) | Expr::Cast { value: inner, .. } => {
                    walk(c, *inner, found)
                }
                Expr::Binary { lhs, rhs, .. } => walk(c, *lhs, found) && walk(c, *rhs, found),
                _ => false,
            }
        }
        let mut found = false;
        walk(self, expr, &mut found) && found
    }

    /// Notes that constant `c`'s value is needed here, if it's worked out by running code
    /// and isn't known yet: the program is checked again once it is. Whether it was.
    pub(super) fn need(&mut self, c: Id<som_ast::Const>) -> bool {
        if self.known.get(c).is_some() || !som_resolve::const_runs_code(self.ast, self.names, c) {
            return false;
        }
        if !self.needed.contains(&c) {
            self.needed.push(c);
        }
        true
    }

    /// Whether `expr` is made of a constant whose value is needed and isn't known yet: what
    /// it's needed for can't be worked out this time round, and isn't an error.
    pub(super) fn needs_unknown(&mut self, expr: Id<Expr>) -> bool {
        let mut found = false;
        let mut todo = vec![expr];
        while let Some(expr) = todo.pop() {
            match &self.ast.exprs[expr] {
                Expr::Var(_) => {
                    if let Some(def) = self.names.expr(expr)
                        && let DefKind::Const(c) = self.names.def(def).kind
                    {
                        if self.need(c) {
                            found = true;
                        } else if self.known.get(c).is_none() {
                            todo.push(self.ast.consts[c].value);
                        }
                    }
                }
                Expr::Binary { lhs, rhs, .. } => todo.extend([*lhs, *rhs]),
                Expr::Not(inner) | Expr::Neg(inner) | Expr::Cast { value: inner, .. } => {
                    todo.push(*inner)
                }
                Expr::Range { start, end } => todo.extend(start.iter().chain(end)),
                _ => {}
            }
        }
        found
    }
}
