use som_ast::{BinOp, Expr, Id, TypeExpr};
use som_resolve::DefKind;

use super::Checker;
use crate::error::TypeError;
use crate::ty::{IntTy, Ty, TypeId};
use crate::unify::VarKind;

impl Checker<'_> {
    pub(super) fn infer_type_comparison(&mut self, lhs: Id<Expr>, rhs: Id<Expr>) -> TypeId {
        let bool = self.u.table.bool;
        for operand in [lhs, rhs] {
            match self.type_operand(operand) {
                Some(ty) => {
                    self.exprs[operand.id] = Some(ty);
                }
                None => self.errors.push(TypeError::TypeNotCompared {
                    span: self.span(if self.is_type_operand(lhs) { lhs } else { rhs }),
                }),
            }
        }
        bool
    }

    pub(super) fn infer_cast(&mut self, id: Id<Expr>, value: Id<Expr>, ty: Id<TypeExpr>) -> TypeId {
        let target = self.lower(ty);
        let found = self.infer(value);
        // A literal converted to an integer type is read in full, as an `i128`, then
        // wrapped: `18446744073709551617 as u64` is 1, `-1 as u64` the largest.
        if matches!(self.ast.exprs[value], Expr::Int(_))
            && self.u.var_kind(found) == Some(VarKind::Int)
            && matches!(self.u.shallow_ty(target), Ty::Int(_))
        {
            let i128 = self.u.intern(Ty::Int(IntTy::I128));
            let _ = self.u.unify(found, i128);
        }
        // The value converts at run time: `300 as u8` is the `i32` 300, wrapped to 44.
        let number = |c: &Self, ty: TypeId| {
            matches!(c.u.shallow_ty(ty), Ty::Int(_) | Ty::Float(_) | Ty::Error)
                || matches!(c.u.var_kind(ty), Some(VarKind::Int | VarKind::Float))
        };
        // An enum whose variants hold nothing converts to the number of its variant.
        let plain_enum = self.is_plain_enum(self.u.shallow_ty(found))
            && matches!(self.u.shallow_ty(target), Ty::Int(_));
        let pointer = |c: &Self, ty: TypeId| matches!(c.u.shallow_ty(ty), Ty::Pointer | Ty::Ptr(_));
        let reference = |c: &Self, ty: TypeId| matches!(c.u.shallow_ty(ty), Ty::Ref { .. });
        let pointers = (pointer(self, found) || reference(self, found)) && pointer(self, target);
        // C's text is a pointer to its bytes: `bytes as cstr`, `text as *u8`.
        let cstr = |c: &Self, ty: TypeId| matches!(c.u.shallow_ty(ty), Ty::CStr);
        let c_text = (cstr(self, found) && pointer(self, target))
            || (pointer(self, found) && cstr(self, target));
        if !plain_enum && !pointers && !c_text && (!number(self, found) || !number(self, target)) {
            self.errors.push(TypeError::InvalidCast {
                from: self.describe(found),
                to: self.describe(target),
                span: self.span(id),
            });
        }
        target
    }

    pub(super) fn infer_binary(
        &mut self,
        id: Id<Expr>,
        op: BinOp,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
    ) -> TypeId {
        let error = self.u.table.error;
        let bool = self.u.table.bool;
        // `x == none` and `x != none` ask whether `x` holds a value.
        let against_none = matches!(op, BinOp::Equals | BinOp::NotEquals)
            && (matches!(self.ast.exprs[lhs], Expr::None)
                || matches!(self.ast.exprs[rhs], Expr::None));
        let (lhs_ty, rhs_ty) = if against_none {
            (self.infer_optional(lhs), self.infer_optional(rhs))
        } else {
            (self.infer(lhs), self.infer(rhs))
        };
        // `name + "!"`: text in quotes is a `string`, with its operators.
        self.text_as_string(lhs_ty);
        self.text_as_string(rhs_ty);
        if against_none {
            // An overloaded call not decided yet: only one giving an optional can be
            // `none`; a plain value would only fit by being wrapped.
            for ty in [lhs_ty, rhs_ty] {
                if self.u.var_kind(ty) == Some(VarKind::Any) {
                    self.decide_pending_by(ty, &|c, ret| {
                        matches!(c.u.shallow_ty(ret), Ty::Optional(_))
                    });
                }
            }
            if self.u.unify(lhs_ty, rhs_ty).is_err() {
                let ((optional, ty), none) = if matches!(self.ast.exprs[lhs], Expr::None) {
                    ((rhs, rhs_ty), lhs)
                } else {
                    ((lhs, lhs_ty), rhs)
                };
                self.errors.push(TypeError::NotOptional {
                    ty: self.describe(ty),
                    span: self.span(optional),
                });
                // Already reported: don't also say the `none` can't be inferred.
                self.exprs[none.id] = Some(error);
            }
            return bool;
        }
        // `x == 5` on an optional: true only if it holds 5.
        if matches!(op, BinOp::Equals | BinOp::NotEquals)
            && let Some((lhs_inner, rhs_inner)) = self.optional_operands(lhs_ty, rhs_ty)
        {
            // The values are compared, but errors name the types as written.
            let comparable = self.u.unify(lhs_inner, rhs_inner).is_ok();
            match self.operand_ok(op, lhs_inner) {
                Some(true) if comparable => {}
                None if comparable => self.deferred.ops.push((id, op, lhs_inner, rhs_inner)),
                // `name == "x"` of a `string?`: the values compare with `Eq`'s `eq`.
                Some(false) if comparable && self.trait_op(id, op, lhs_inner).is_some() => {}
                _ => self.invalid_operands(id, op, lhs_ty, rhs_ty),
            }
            return bool;
        }
        let lhs_ty = self.through_reference(lhs, lhs_ty);
        let rhs_ty = self.through_reference(rhs, rhs_ty);
        self.binary(id, op, lhs_ty, rhs_ty)
    }

    pub(super) fn infer_bit_not(&mut self, id: Id<Expr>, inner: Id<Expr>) -> TypeId {
        let ty = self.infer(inner);
        let shallow = self.u.shallow(ty);
        let integer = matches!(self.u.get(shallow), Ty::Int(_) | Ty::Error)
            || self.u.var_kind(shallow) == Some(VarKind::Int);
        if !integer {
            self.errors.push(TypeError::CannotFlipBits {
                ty: self.describe(ty),
                span: self.span(id),
            });
            return self.u.table.error;
        }
        ty
    }

    pub(super) fn infer_neg(&mut self, id: Id<Expr>, inner: Id<Expr>) -> TypeId {
        let ty = self.infer(inner);
        let shallow = self.u.shallow(ty);
        let negatable = match (self.u.get(shallow), self.u.var_kind(shallow)) {
            (Ty::Int(int), _) => int.is_signed(),
            (Ty::Float(_) | Ty::Error, _) => true,
            (_, Some(VarKind::Int | VarKind::Float)) => true,
            _ => false,
        };
        if !negatable {
            // `-v` through an implementation of `Negate`.
            if let Some(output) = self.negate_impl(id, shallow) {
                return output;
            }
            self.errors.push(TypeError::CannotNegate {
                ty: self.describe(ty),
                span: self.span(id),
            });
            return self.u.table.error;
        }
        ty
    }

    /// What `-x` of a `ty` gives through `Negate`, if `ty` has it: recorded for `id`, with the
    /// trait's type arguments.
    fn negate_impl(&mut self, id: Id<Expr>, ty: TypeId) -> Option<TypeId> {
        let trait_ = self.names.lang(som_lang::LangTrait::Negate)?;
        let output = match self.u.get(ty).clone() {
            // `T: Negate` is `T: Negate<T>`.
            Ty::Param(def) => self.names.bounds(def).contains(&trait_).then_some(ty)?,
            _ => {
                let u = &self.u;
                let (index, args) =
                    crate::impls::find_impl(&self.impls, &u.table, trait_, ty, &[], &|t| u.shallow(t))?;
                match self.impls[index].trait_args.first().copied() {
                    Some(output) => {
                        let subst = self.impls[index].subst(&args);
                        crate::impls::substitute(&mut self.u.table, output, &subst)
                    }
                    None => ty,
                }
            }
        };
        self.records.trait_ops.insert(id, som_lang::LangTrait::Negate);
        self.type_args.insert(id, vec![ty, output]);
        Some(output)
    }

    fn binary(&mut self, id: Id<Expr>, op: BinOp, lhs: TypeId, rhs: TypeId) -> TypeId {
        let table = &self.u.table;
        let (error, bool, pointer) = (table.error, table.bool, table.pointer);

        if op == BinOp::Add && self.u.shallow(lhs) == pointer {
            let int = self.u.fresh(VarKind::Int);
            if self.u.unify(int, rhs).is_err() {
                self.invalid_operands(id, op, lhs, rhs);
            }
            return pointer;
        }

        if matches!(op, BinOp::And | BinOp::Or) {
            if self.u.unify(lhs, bool).is_err() || self.u.unify(rhs, bool).is_err() {
                self.invalid_operands(id, op, lhs, rhs);
            }
            return bool;
        }

        // The shift amount can be any integer type.
        if matches!(op, BinOp::ShiftLeft | BinOp::ShiftRight) {
            let amount = self.u.fresh(VarKind::Int);
            if self.u.unify(amount, rhs).is_err() {
                self.invalid_operands(id, op, lhs, rhs);
                return error;
            }
            match self.operand_ok(op, lhs) {
                Some(true) => {}
                Some(false) => self.invalid_operands(id, op, lhs, rhs),
                None => self.deferred.ops.push((id, op, lhs, lhs)),
            }
            return lhs;
        }

        // `"ab" * 3`, `money + money`: the operator trait's impl says what comes out.
        if let Some(lang) = op.lang_trait()
            && let Some(output) = self.operator_impl(id, lang, lhs, rhs)
        {
            return output;
        }

        // `"score: " + n`: an operand of another type than one whose operator is a trait's
        // method becomes that type, through `From`, as an argument would.
        let (mut lhs, mut rhs) = (lhs, rhs);
        if let Expr::Binary {
            lhs: left,
            rhs: right,
            ..
        } = self.ast.exprs[id]
            && let Some(lang) = op.lang_trait()
            && !self.u.could_fit(lhs, rhs)
        {
            let has_op = |c: &mut Self, ty: TypeId| {
                let ty = c.u.shallow(ty);
                matches!(c.u.get(ty), Ty::Struct(..)) && c.self_operator(ty, lang).is_some()
            };
            // As the operand is, before being read through: `r` of a `&i32` as a `&i32`.
            let own = |c: &Self, side: Id<Expr>, ty: TypeId| c.exprs[side.id].unwrap_or(ty);
            let (left_own, right_own) = (own(self, left, lhs), own(self, right, rhs));
            if has_op(self, lhs) && self.from_conversion(right, lhs, right_own) {
                rhs = lhs;
            } else if has_op(self, rhs) && self.from_conversion(left, rhs, left_own) {
                lhs = rhs;
            }
        }
        let result = if op.is_arithmetic() { lhs } else { bool };
        if self.u.unify(lhs, rhs).is_err() {
            self.invalid_operands(id, op, lhs, rhs);
            return if op.is_arithmetic() { error } else { bool };
        }
        // Values of a type implementing `Eq` compare with its `eq`, `Add` add with its `add`.
        if let Some(output) = self.trait_op(id, op, lhs) {
            return if op.is_arithmetic() { output } else { bool };
        }
        match self.operand_ok(op, lhs) {
            Some(true) => {}
            Some(false) => self.invalid_operands(id, op, lhs, rhs),
            None => self.deferred.ops.push((id, op, lhs, rhs)),
        }
        result
    }

    /// A reference to a number, `bool` or struct is read through, however many there are: so
    /// `self * self` works for a `&i32`, `"arg: " + arg` for a `&string`, and `x % 2` for the
    /// `&&i32` a `where` on lent values gives.
    fn through_reference(&mut self, operand: Id<Expr>, ty: TypeId) -> TypeId {
        let mut to = self.u.shallow(ty);
        let mut depth = 0;
        while let Ty::Ref { to: inner, .. } = *self.u.get(to) {
            to = self.u.shallow(inner);
            depth += 1;
        }
        let readable = matches!(self.u.get(to), Ty::Int(_) | Ty::Float(_) | Ty::Bool)
            || matches!(self.u.var_kind(to), Some(VarKind::Int | VarKind::Float))
            || matches!(self.u.get(to), Ty::Struct(..));
        if depth == 0 || !readable {
            return ty;
        }
        self.records.read_through.insert(operand, depth);
        to
    }

    /// Variants holding nothing: a number, really, which can be compared and converted to one.
    fn is_plain_enum(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Struct(def, _) => match self.names.def(*def).kind {
                DefKind::Struct(s) => {
                    let s = &self.ast.structs[s];
                    s.is_enum() && s.payload().next().is_none()
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether `op` is defined on operands of type `ty`, or `None` if `ty` isn't known yet.
    pub(super) fn operand_ok(&self, op: BinOp, ty: TypeId) -> Option<bool> {
        if let Some(kind) = self.u.var_kind(ty) {
            return match kind {
                VarKind::Any => None,
                VarKind::Int => Some(true),
                VarKind::Float => Some(!op.is_bitwise()),
                VarKind::Collection { .. } | VarKind::Text => Some(false),
            };
        }
        let ty = self.u.shallow_ty(ty);
        Some(match op {
            _ if matches!(ty, Ty::Error) => true,
            BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide | BinOp::Modulo => {
                matches!(ty, Ty::Int(_) | Ty::Float(_))
            }
            BinOp::LessThan
            | BinOp::GreaterThan
            | BinOp::LessThanOrEqual
            | BinOp::GreaterThanOrEqual => {
                matches!(ty, Ty::Int(_) | Ty::Float(_) | Ty::Char)
            }
            BinOp::Equals | BinOp::NotEquals => {
                matches!(
                    ty,
                    Ty::Int(_) | Ty::Float(_) | Ty::Char | Ty::Bool | Ty::Pointer
                ) || self.is_plain_enum(ty)
            }
            BinOp::And | BinOp::Or => matches!(ty, Ty::Bool),
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
                matches!(ty, Ty::Int(_))
            }
        })
    }
}
