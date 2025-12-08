use crate::{ExprId, Ident, Type};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StmtId(pub u32);

pub enum Stmt {
    Let {
        name: Ident,
        ty: Option<Type>,
        value: ExprId,
    },
}
