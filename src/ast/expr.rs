use crate::{FuncId, Ident, StmtId, TraitId};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

pub enum Expr {
    /// hole caused by invalid/error code
    Hole,
    I32(i32),
    Bool(bool),
    String(Box<str>),
    Var(Ident),
    Binary {
        op: BinOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Block {
        stmts: Vec<StmtId>,
        value: Option<ExprId>,
    },
    Call {
        func: FuncId,
        args: Vec<ExprId>,
    },
    Borrow {
        mutable: bool,
        expr: ExprId,
    },
    Deref {
        expr: ExprId,
    },
}

pub enum BinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    LessThan,
    GreaterThan,
    Equals,
    NotEquals,
    And,
    Or,
}

pub const TRAIT_ADD: TraitId = TraitId(0);
pub const TRAIT_SUB: TraitId = TraitId(1);
pub const TRAIT_MUL: TraitId = TraitId(2);
pub const TRAIT_DIV: TraitId = TraitId(3);
pub const TRAIT_EQ: TraitId = TraitId(4);
pub const TRAIT_NEQ: TraitId = TraitId(5);
pub const TRAIT_LT: TraitId = TraitId(6);
pub const TRAIT_GT: TraitId = TraitId(7);
pub const TRAIT_LT_EQ: TraitId = TraitId(8);
pub const TRAIT_GT_EQ: TraitId = TraitId(9);
pub const TRAIT_AND: TraitId = TraitId(10);
pub const TRAIT_OR: TraitId = TraitId(11);
impl BinOp {
    pub fn trait_id(&self) -> TraitId {
        match self {
            BinOp::Add => TRAIT_ADD,
            BinOp::Subtract => TRAIT_SUB,
            BinOp::Multiply => TRAIT_MUL,
            BinOp::Divide => TRAIT_DIV,
            BinOp::LessThan => TRAIT_LT,
            BinOp::GreaterThan => TRAIT_GT,
            BinOp::Equals => TRAIT_EQ,
            BinOp::NotEquals => TRAIT_NEQ,
            BinOp::And => TRAIT_AND,
            BinOp::Or => TRAIT_OR,
        }
    }
}
