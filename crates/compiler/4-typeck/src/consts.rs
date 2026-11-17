use std::collections::HashMap;

use som_ast::{Ast, BinOp, Expr, Id};
use som_resolve::{DefKind, Names};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConstValue {
    Int(i128),
    Bool(bool),
    /// A struct, text: not usable as a count or a condition.
    Other,
}

#[derive(Clone, Debug, Default)]
pub struct ConstValues(pub HashMap<Id<som_ast::Const>, ConstValue>);

impl ConstValues {
    pub fn get(&self, c: Id<som_ast::Const>) -> Option<ConstValue> {
        self.0.get(&c).copied()
    }
}

/// `None` if it isn't an integer, or doesn't fit.
pub fn const_number(ast: &Ast, names: &Names, expr: Id<Expr>) -> Option<i128> {
    const_number_with(ast, names, &ConstValues::default(), expr)
}

pub fn const_number_with(
    ast: &Ast,
    names: &Names,
    known: &ConstValues,
    expr: Id<Expr>,
) -> Option<i128> {
    let number = |expr| const_number_with(ast, names, known, expr);
    Some(match ast.exprs[expr] {
        Expr::Int(n) => n,
        Expr::Neg(inner) => number(inner)?.checked_neg()?,
        Expr::Cast { value, .. } => number(value)?,
        Expr::Var(ref path) if let Some((_, n)) = path.int_limit() => n,
        Expr::Var(_) => {
            let def = names.expr(expr)?;
            let DefKind::Const(c) = names.def(def).kind else {
                return None;
            };
            match known.get(c) {
                Some(ConstValue::Int(n)) => n,
                Some(_) => return None,
                None => number(ast.consts[c].value)?,
            }
        }
        Expr::Binary { op, lhs, rhs } => fold(op, number(lhs)?, number(rhs)?)?,
        _ => return None,
    })
}

/// `None` if it overflows, or `op` isn't arithmetic.
pub(crate) fn fold(op: BinOp, a: i128, b: i128) -> Option<i128> {
    Some(match op {
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
    })
}

pub(crate) fn compare(op: BinOp, a: i128, b: i128) -> Option<bool> {
    Some(match op {
        BinOp::Equals => a == b,
        BinOp::NotEquals => a != b,
        BinOp::LessThan => a < b,
        BinOp::GreaterThan => a > b,
        BinOp::LessThanOrEqual => a <= b,
        BinOp::GreaterThanOrEqual => a >= b,
        _ => return None,
    })
}
