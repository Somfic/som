use crate::{Expr, Type};
use som_common::Id;

pub enum Constraint {
    Equal {
        provenance: Provenance,
        expected: Id<Type>,
        actual: Id<Type>,
    },
}

pub enum Provenance {
    BinaryOp(Id<Expr>),
    Unary(Id<Expr>),
    Check(Id<Expr>),
}

impl Provenance {
    pub fn expr(&self) -> Id<Expr> {
        match self {
            Provenance::BinaryOp(id) | Provenance::Unary(id) | Provenance::Check(id) => *id,
        }
    }
}
