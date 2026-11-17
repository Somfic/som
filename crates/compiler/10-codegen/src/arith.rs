use crate::Emitter;
use crate::c_types::{c_int, portable_bits};
use crate::fold::{Expr, FnState, Prec};
use som_mir::*;

impl Emitter<'_> {
    /// `x << n` or `x >> n`. By a constant less than `x`'s width, C's own shift, through the
    /// unsigned type for a signed `<<`, which C leaves undefined for negative values; by
    /// anything else, a helper.
    pub(crate) fn shift(
        &self,
        f: &FnState,
        op: BinOp,
        x_op: &Operand,
        n_op: &Operand,
        x: Expr,
        n: Expr,
    ) -> Expr {
        let x_ty = self.operand_ty(f, x_op);
        let n_ty = self.operand_ty(f, n_op);
        let Ty::Int(int) = *self.mir.types.get(x_ty) else {
            unreachable!("checked: only integers shift")
        };
        let width = portable_bits(int) as i128;
        if let Operand::Const(Const {
            kind: ConstKind::Int(amount),
            ..
        }) = n_op
            && (0..width).contains(amount)
        {
            let n = n.text;
            let text = match (op, int.is_signed()) {
                (BinOp::ShiftLeft, true) => {
                    let (c_ty, unsigned) = (self.ty(x_ty), c_int(int).unsigned);
                    format!("({c_ty})(({unsigned}){} << {n})", x.operand())
                }
                (BinOp::ShiftLeft, false) => format!("{} << {n}", x.operand_of(op, true)),
                _ => format!("{} >> {n}", x.operand_of(op, true)),
            };
            // A small type's shift is worked out as an `int`: it's cut back where it's used.
            let prec = if self.is_narrow(x_ty) || int.is_signed() && op == BinOp::ShiftLeft {
                Prec::Other
            } else {
                Prec::Shift
            };
            return Expr { text, prec };
        }
        let helper = self.shift_helper(op, x_ty, n_ty);
        Expr::atom(format!("{helper}({}, {})", x.text, n.text))
    }

    /// A float converted to the integer type `int`: C leaves one out of range undefined, so
    /// it saturates, and NaN is 0. `x` is read more than once, which only ever means reading a
    /// place again: calls aren't folded into conversions.
    pub(crate) fn float_to_int(&self, x: &str, int: IntTy) -> Expr {
        let c = c_int(int);
        let target = c.name;
        // C can't write the 128-bit types' limits as literals; these convert unchecked.
        let Some((min, max)) = c.limits else {
            return Expr::compound(format!("({target}){x}"));
        };
        // Comparing in `double`: a limit that rounds up there still compares right, as anything
        // that large or larger is out of range.
        Expr::compound(format!(
            "({x} != {x} ? ({target})0 : {x} <= (double){min} ? ({target}){min} \
             : {x} >= (double){max} ? ({target}){max} : ({target}){x})"
        ))
    }

    /// `n` as a C literal of type `ty`. C has no literal past 64 bits, and none for the
    /// smallest `long long`: `-9223372036854775808` is `-` applied to a number too big for
    /// one. Above the largest `long long`, it's unsigned.
    pub(crate) fn int_literal(&self, n: i128, ty: TypeId) -> String {
        // `usize` and `isize` are as wide as a pointer: their limits are the target's own.
        if let Ty::Int(int @ (IntTy::ISize | IntTy::USize)) = *self.mir.types.get(ty)
            && let Some((min, max)) = c_int(int).limits
        {
            if n == int.max() as i128 {
                return max.into();
            }
            if int.is_signed() && n == int.min() {
                return min.into();
            }
        }
        if n == i64::MIN as i128 {
            return "(-9223372036854775807LL - 1)".into();
        }
        if i64::try_from(n).is_ok() {
            return n.to_string();
        }
        if u64::try_from(n).is_ok() {
            return format!("{n}ULL");
        }
        // Past 64 bits: its two halves, put together.
        let bits = n as u128;
        let (high, low) = ((bits >> 64) as u64, bits as u64);
        format!(
            "(({})(((unsigned __int128){high}ULL << 64) | {low}ULL))",
            self.ty(ty)
        )
    }
}
