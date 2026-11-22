//! Expressions as what facts are made of: conditions, and integer values of places.

use som_ast::{BinOp, Expr, Id, Ident, Stmt};
use som_resolve::{DefId, DefKind};
use som_typeck::Ty;

use crate::facts::{Compare, Cond, Term, Value};
use crate::{Place, Prover, Subst, as_is};

impl Prover<'_> {
    /// What `expr` says when it's `truth`: `i < xs.len` when true, `i >= xs.len` when false.
    pub(crate) fn cond(&mut self, expr: Id<Expr>, truth: bool, subst: Subst) -> Cond<Place> {
        match self.ast.exprs[expr] {
            Expr::Bool(value) => match value == truth {
                true => Cond::always(),
                false => Cond::never(),
            },
            Expr::Not(inner) => self.cond(inner, !truth, subst),
            Expr::Binary { op, lhs, rhs } => self.binary_cond(op, lhs, rhs, truth, subst),
            Expr::Var(_) | Expr::Field { .. } if self.is_bool(expr) => {
                match self
                    .value_of(expr, subst)
                    .and_then(|(value, _)| into_place(value))
                {
                    Some(place) => Cond::Is(place, truth),
                    None => Cond::Unknown,
                }
            }
            _ => Cond::Unknown,
        }
    }

    fn binary_cond(
        &mut self,
        op: BinOp,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
        truth: bool,
        subst: Subst,
    ) -> Cond<Place> {
        let compare = match op {
            BinOp::And | BinOp::Or => {
                let both = vec![self.cond(lhs, truth, subst), self.cond(rhs, truth, subst)];
                // `a and b` being false is `!a or !b`.
                return match (op == BinOp::And) == truth {
                    true => Cond::All(both),
                    false => Cond::Any(both),
                };
            }
            BinOp::LessThan => Compare::Less,
            BinOp::LessThanOrEqual => Compare::LessOrEqual,
            BinOp::GreaterThan => Compare::Greater,
            BinOp::GreaterThanOrEqual => Compare::GreaterOrEqual,
            BinOp::Equals => Compare::Equal,
            BinOp::NotEquals => Compare::NotEqual,
            _ => return Cond::Unknown,
        };
        if !self.is_integer(lhs) {
            return Cond::Unknown;
        }
        let compare = if truth { compare } else { compare.opposite() };
        let (Some((x, x_needs)), Some((y, y_needs))) =
            (self.value_of(lhs, subst), self.value_of(rhs, subst))
        else {
            return Cond::Unknown;
        };
        let cond = Cond::compare(&x, compare, &y);
        // `xs.len - 1` is only one less than `xs.len` where it doesn't wrap.
        let needs: Vec<_> = x_needs.into_iter().chain(y_needs).collect();
        match needs.is_empty() {
            true => cond,
            false => Cond::All(needs.into_iter().chain([cond]).collect()),
        }
    }

    pub(crate) fn all_hold(&self, needs: &[Cond<Place>]) -> bool {
        needs.iter().all(|n| self.facts.holds(n))
    }

    /// `expr`'s value, where what it needs not to wrap around is known to hold.
    pub(crate) fn known_value(&mut self, expr: Id<Expr>) -> Option<Value<Place>> {
        let (value, needs) = self.value_of(expr, &as_is)?;
        self.all_hold(&needs).then_some(value)
    }

    /// `expr` as a value followed, if it's one: a number written out; a variable, field or
    /// property of one; an array's length; or one of those plus or minus a number. With
    /// what has to hold for it not to wrap around: `xs.len - 1` of a `usize` needs
    /// `xs.len >= 1`.
    pub(crate) fn value_of(
        &mut self,
        expr: Id<Expr>,
        subst: Subst,
    ) -> Option<(Value<Place>, Vec<Cond<Place>>)> {
        // Worked out while compiling: `3 * W + 5`.
        if let Some(n) = self.const_int(expr) {
            return Some((Value::number(n), Vec::new()));
        }
        let value = match self.ast.exprs[expr] {
            Expr::Int(n) => Value::number(n),
            Expr::Var(_) => {
                let def = self.names.expr(expr)?;
                if let Some(value) = subst(def) {
                    return Some((value, Vec::new()));
                }
                self.var_value(def)?
            }
            Expr::Field { object, ref field } => self.field_value(expr, object, field, subst)?,
            Expr::Deref(inner) => return self.value_of(inner, subst),
            // `width()` of `fn width() -> i32 = 60;`.
            Expr::Call { .. } => Value::number(self.constant_call(expr)?),
            Expr::Binary {
                op: op @ (BinOp::Add | BinOp::Subtract),
                lhs,
                rhs,
            } => return self.sum_value(expr, op, lhs, rhs, subst),
            // `key as usize` of a `u32`: the same number, which every value of it fits.
            Expr::Cast { value, .. } if self.widens(value, expr) => return self.value_of(value, subst),
            _ => return None,
        };
        if self.is_unsigned(expr)
            && value.minus == Term::Zero
            && let Term::Place(place) = &value.term
        {
            self.facts.unsigned(place.clone());
        }
        Some((value, Vec::new()))
    }

    /// Whether casting `from` to `to`'s type keeps every value as it is: an integer to one
    /// at least as wide, of the same sign, or unsigned to a wider signed one. `usize` counts
    /// as 32 bits here, the least it can be.
    fn widens(&self, from: Id<Expr>, to: Id<Expr>) -> bool {
        let int = |expr| match self.expr_ty(expr).map(|ty| self.typed.ty(self.through_ref(ty))) {
            Some(&Ty::Int(int)) => Some(int),
            _ => None,
        };
        let (Some(from), Some(to)) = (int(from), int(to)) else {
            return false;
        };
        let least = |int: som_lang::IntTy| match int {
            som_lang::IntTy::USize | som_lang::IntTy::ISize => 32,
            _ => int.bits(),
        };
        match (from.is_signed(), to.is_signed()) {
            (a, b) if a == b => from.bits() <= least(to),
            (false, true) => from.bits() < least(to),
            _ => false,
        }
    }

    fn var_value(&mut self, def: DefId) -> Option<Value<Place>> {
        let value = match self.names.def(def).kind {
            DefKind::Local(_) | DefKind::Param { .. } | DefKind::Binding(_) => {
                Value::place(Place::var(def))
            }
            // `W`, of `@const W = 60;`; or, for one whose value runs code, worked out after
            // this, a value nothing changes: what's checked of it holds.
            DefKind::Const(c) => match self.const_int(self.ast.consts[c].value) {
                Some(n) => Value::number(n),
                None => Value::place(Place::var(def)),
            },
            // `N`, proving one instance: the number it stands for there.
            DefKind::TypeParam { value: true, .. }
            | DefKind::StructParam { value: true, .. }
            | DefKind::ImplParam { value: true, .. }
                if let Some(&n) = self.bound.get(&def) =>
            {
                Value::number(n)
            }
            // `N`, of `[T; N]`.
            DefKind::TypeParam { value: true, .. }
            | DefKind::StructParam { value: true, .. }
            | DefKind::ImplParam { value: true, .. } => {
                self.facts.unsigned(Place::Len(def));
                Value::place(Place::Len(def))
            }
            _ => return None,
        };
        Some(value)
    }

    fn field_value(
        &mut self,
        expr: Id<Expr>,
        object: Id<Expr>,
        field: &Ident,
        subst: Subst,
    ) -> Option<Value<Place>> {
        let is_len = &*field.name == som_lang::LEN;
        if is_len && let Some(len) = self.array_len(object) {
            return Some(len);
        }
        if let Some(value) = self.prop_value(expr, object, subst) {
            return Some(value);
        }
        let (base, _) = self.value_of(object, subst)?;
        let (root, mut path) = base.into_path()?;
        path.push(field.name.clone());
        let place = Place::Path { root, path };
        // A slice's length is never below 0, whatever integer type it's read as.
        if is_len {
            let mut ty = self.typed.expr(object);
            while let Ty::Ref { to, .. } = *self.typed.ty(ty) {
                ty = to;
            }
            if matches!(self.typed.ty(ty), Ty::Slice(_) | Ty::Str) {
                self.facts.unsigned(place.clone());
            }
        }
        Some(Value::place(place))
    }

    /// `object.name` of a property whose body is a value of what it's read on: `3` of `prop
    /// len: usize = 3`, `xs.mem.size` of `prop capacity = .mem.size`. A trait's, read in its
    /// `@where`s, is the implementation's, for the method being proven.
    fn prop_value(
        &mut self,
        expr: Id<Expr>,
        object: Id<Expr>,
        subst: Subst,
    ) -> Option<Value<Place>> {
        const DEEPEST: usize = 8;
        let DefKind::Func(mut prop) = self.names.def(self.typed.method(expr)?).kind else {
            return None;
        };
        if !self.ast.funcs[prop].prop || self.prop_depth >= DEEPEST {
            return None;
        }
        if self.ast.funcs[prop].in_trait {
            let owner = self.names.impls[self.impl_block?].owner?;
            let name = &self.ast.funcs[prop].name.name;
            let own = *self.names.props(owner, name).first()?;
            let DefKind::Func(own) = self.names.def(own).kind else {
                return None;
            };
            prop = own;
        }
        let body = self.ast.funcs[prop].body?;
        let this = *self.names.params(prop).first()?;
        let (base, _) = self.value_of(object, subst)?;
        let through = move |def: DefId| (def == this).then(|| base.clone());
        self.prop_depth += 1;
        let value = self.value_of(body, &through);
        self.prop_depth -= 1;
        Some(value?.0)
    }

    /// `a + b` or `a - b`, of a number and a value, or of two values.
    fn sum_value(
        &mut self,
        expr: Id<Expr>,
        op: BinOp,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
        subst: Subst,
    ) -> Option<(Value<Place>, Vec<Cond<Place>>)> {
        let (n, other) = match (&self.ast.exprs[lhs], &self.ast.exprs[rhs]) {
            (_, &Expr::Int(n)) => (n, lhs),
            (&Expr::Int(n), _) if op == BinOp::Add => (n, rhs),
            // `N - 1 - i`: one term less another.
            _ => {
                let (a, mut needs) = self.value_of(lhs, subst)?;
                let (b, more) = self.value_of(rhs, subst)?;
                needs.extend(more);
                let value = match op {
                    BinOp::Add => a.plus(&b)?,
                    _ => {
                        // `a - b` doesn't wrap where `a >= b`.
                        if self.is_unsigned(expr) {
                            needs.push(Cond::compare(&a, Compare::GreaterOrEqual, &b));
                        }
                        a.less(&b)?
                    }
                };
                return Some((value, needs));
            }
        };
        let (mut value, mut needs) = self.value_of(other, subst)?;
        if op == BinOp::Add {
            value.offset += n;
        } else {
            if self.is_unsigned(expr) {
                // `v - n` doesn't wrap where `v >= n`.
                needs.push(Cond::compare(
                    &value,
                    Compare::GreaterOrEqual,
                    &Value::number(n),
                ));
            }
            value.offset -= n;
        }
        Some((value, needs))
    }

    /// The length of array `object`, if it is one: a number, or its length parameter.
    pub(crate) fn array_len(&mut self, object: Id<Expr>) -> Option<Value<Place>> {
        let mut ty = self.typed.expr(object);
        if let Ty::Ref { to, .. } = *self.typed.ty(ty) {
            ty = to;
        }
        let Ty::Array { len, .. } = *self.typed.ty(ty) else {
            return None;
        };
        match *self.typed.ty(len) {
            Ty::Value(som_typeck::Value::Int(n)) => Some(Value::number(n as i128)),
            Ty::Param(def) => {
                self.facts.unsigned(Place::Len(def));
                Some(Value::place(Place::Len(def)))
            }
            _ => None,
        }
    }

    /// The length of what `object` is: an array's, or `.len` of the variable or field it is.
    pub(crate) fn len_of(&mut self, object: Id<Expr>) -> Option<Value<Place>> {
        if let Some(len) = self.array_len(object) {
            return Some(len);
        }
        let (base, _) = self.value_of(object, &as_is)?;
        let (root, mut path) = base.into_path()?;
        path.push(som_lang::LEN.into());
        Some(Value::place(Place::Path { root, path }))
    }

    /// The least and most integer `expr` can be: from what's known of each place in it
    /// (or as far as its type goes), through `+`, `-`, `*`, `/`, `%` and negation. What's compared
    /// with each other isn't followed here (that's [`Facts`](crate::facts::Facts)), only how
    /// far each can go: `y * 10 + x` of `x` from 0 to 9 and `y` from 0 to 19 is from 0 to 199.
    /// `None` where a step could wrap around, as its type has no room for it.
    pub(crate) fn range_of(&mut self, expr: Id<Expr>) -> Option<(i128, i128)> {
        if !self.is_integer(expr) {
            return None;
        }
        let (least, most) = match self.ast.exprs[expr] {
            Expr::Neg(inner) => {
                let (least, most) = self.range_of(inner)?;
                (most.checked_neg()?, least.checked_neg()?)
            }
            Expr::Binary { op, lhs, rhs }
                if matches!(
                    op,
                    BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide | BinOp::Modulo
                ) =>
            {
                let (a, b) = (self.range_of(lhs)?, self.range_of(rhs)?);
                arithmetic_range(op, a, b)?
            }
            _ => {
                let value = self.known_value(expr)?;
                if value.is_number() {
                    (value.offset, value.offset)
                } else {
                    // `m - t <= d` is `t - m >= -d`; `t - m <= d` is `t - m <= d`. Where
                    // one isn't known, it's as far as the type goes.
                    let (term, minus) = (&value.term, &value.minus);
                    let (min, max) = self.int_limits(expr)?;
                    let least = match self.facts.bound(minus, term) {
                        Some(below) => value.offset.checked_sub(below)?,
                        None => min,
                    };
                    let most = match self.facts.bound(term, minus) {
                        Some(above) => value.offset.checked_add(above)?,
                        None => max,
                    };
                    (least, most)
                }
            }
        };
        let (min, max) = self.int_limits(expr)?;
        (min <= least && most <= max).then_some((least, most))
    }

    /// The least and most a value of `expr`'s integer type can be.
    fn int_limits(&self, expr: Id<Expr>) -> Option<(i128, i128)> {
        let ty = self.through_ref(self.expr_ty(expr)?);
        let Ty::Int(int) = *self.typed.ty(ty) else {
            return None;
        };
        Some((int.min(), int.max().min(i128::MAX as u128) as i128))
    }

    /// The integer a constant's value is: `W * H` of `@const W = 60;` and `@const H = 20;`
    /// is 1200. `None` where it isn't one, or a step wraps around.
    fn const_int(&self, expr: Id<Expr>) -> Option<i128> {
        let n = match self.ast.exprs[expr] {
            Expr::Int(n) => n,
            Expr::Neg(inner) => self.const_int(inner)?.checked_neg()?,
            Expr::BitNot(inner) => !self.const_int(inner)?,
            Expr::Cast { value, .. } => self.const_int(value)?,
            Expr::Var(ref path) if let Some((_, n)) = path.int_limit() => n,
            Expr::Var(_) => {
                let def = self.names.expr(expr)?;
                let DefKind::Const(c) = self.names.def(def).kind else {
                    return None;
                };
                // Worked out by running it, before types were checked.
                match self.typed.const_value(c) {
                    Some(som_typeck::ConstValue::Int(n)) => n,
                    Some(_) => return None,
                    None => self.const_int(self.ast.consts[c].value)?,
                }
            }
            Expr::Binary { op, lhs, rhs } => {
                let (a, b) = (self.const_int(lhs)?, self.const_int(rhs)?);
                match op {
                    BinOp::Add => a.checked_add(b)?,
                    BinOp::Subtract => a.checked_sub(b)?,
                    BinOp::Multiply => a.checked_mul(b)?,
                    BinOp::Divide => a.checked_div(b)?,
                    BinOp::Modulo => a.checked_rem(b)?,
                    BinOp::BitAnd => a & b,
                    BinOp::BitOr => a | b,
                    BinOp::BitXor => a ^ b,
                    BinOp::ShiftLeft => a.checked_shl(u32::try_from(b).ok()?)?,
                    BinOp::ShiftRight => a.checked_shr(u32::try_from(b).ok()?)?,
                    _ => return None,
                }
            }
            _ => return None,
        };
        // Within its type, or it wrapped around on the way.
        let (min, max) = self.int_limits(expr)?;
        (min <= n && n <= max).then_some(n)
    }

    /// What a call of a function without parameters always gives, if its body is only a
    /// number: `fn width() -> i32 = 60;`, or `{ return 60; }`.
    fn constant_call(&self, expr: Id<Expr>) -> Option<i128> {
        let Expr::Call { ref args, .. } = self.ast.exprs[expr] else {
            return None;
        };
        if !args.is_empty() {
            return None;
        }
        let func = &self.ast.funcs[self.callee(expr)?];
        if !func.params.is_empty() || !func.generics.is_empty() {
            return None;
        }
        let mut body = func.body?;
        loop {
            body = match self.ast.exprs[body] {
                Expr::Int(n) => return Some(n),
                Expr::Neg(inner) => match self.ast.exprs[inner] {
                    Expr::Int(n) => return Some(-n),
                    _ => return None,
                },
                Expr::Block { ref stmts, value } => match (&stmts[..], value) {
                    ([], Some(value)) => value,
                    ([only], None) => match self.ast.stmts[*only] {
                        Stmt::Return(Some(value)) => value,
                        _ => return None,
                    },
                    _ => return None,
                },
                _ => return None,
            };
        }
    }
}

/// The place a value is, as it is: not `i + 1`.
fn into_place(value: Value<Place>) -> Option<Place> {
    match value {
        Value {
            term: Term::Place(place),
            minus: Term::Zero,
            offset: 0,
        } => Some(place),
        _ => None,
    }
}

/// The range of `a op b`, of `a` and `b` in ranges `a` and `b`. `None` where it could wrap.
fn arithmetic_range(op: BinOp, a: (i128, i128), b: (i128, i128)) -> Option<(i128, i128)> {
    // Of `*` and `/`, the extremes are among the ends' combinations.
    let corners = |f: fn(i128, i128) -> Option<i128>| {
        let all = [f(a.0, b.0)?, f(a.0, b.1)?, f(a.1, b.0)?, f(a.1, b.1)?];
        Some((*all.iter().min()?, *all.iter().max()?))
    };
    match op {
        BinOp::Add => Some((a.0.checked_add(b.0)?, a.1.checked_add(b.1)?)),
        BinOp::Subtract => Some((a.0.checked_sub(b.1)?, a.1.checked_sub(b.0)?)),
        BinOp::Multiply => corners(i128::checked_mul),
        // Rounding towards 0, as C does; never by something that can be 0.
        BinOp::Divide if b.0 > 0 || b.1 < 0 => corners(i128::checked_div),
        // What's left over is below the divisor, and of something not below 0, not below 0
        // either.
        BinOp::Modulo if a.0 >= 0 && b.0 > 0 => Some((0, a.1.min(b.1 - 1))),
        _ => None,
    }
}
