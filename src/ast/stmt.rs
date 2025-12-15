use crate::{ExprId, Ident, Type};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StmtId(pub u32);

pub enum Stmt {
    Let {
        name: Ident,
        mutable: bool,
        ty: Option<Type>,
        value: ExprId,
    },
    Expr {
        expr: ExprId,
    },
}
