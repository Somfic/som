use crate::{Expr, Ident, Type, arena::Id};

pub enum Stmt {
    Let {
        name: Ident,
        mutable: bool,
        ty: Option<Type>,
        value: Id<Expr>,
    },
    Expr {
        expr: Id<Expr>,
    },
}
