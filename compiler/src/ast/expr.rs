use crate::{Ident, Path, Stmt, Trait, arena::Id};

pub enum Expr {
    /// hole caused by invalid/error code
    Hole,
    I32(i32),
    F32(f32),
    Bool(bool),
    String(Box<str>),
    Var(Path),
    Binary {
        op: BinOp,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
    },
    Block {
        stmts: Vec<Id<Stmt>>,
        value: Option<Id<Expr>>,
    },
    Call {
        name: Path,
        args: Vec<Id<Expr>>,
    },
    Borrow {
        mutable: bool,
        expr: Id<Expr>,
    },
    Deref {
        expr: Id<Expr>,
    },
    Not {
        expr: Id<Expr>,
    },
    Conditional {
        condition: Id<Expr>,
        truthy: Id<Expr>,
        falsy: Id<Expr>,
    },
    Constructor {
        struct_name: Path,
        fields: Vec<(Ident, Id<Expr>)>,
    },
    FieldAccess {
        object: Id<Expr>,
        field: Ident,
    },
    Assignment {
        target: Id<Expr>,
        value: Id<Expr>,
    },
    MethodCall {
        object: Id<Expr>,
        method: Ident,
        args: Vec<Id<Expr>>,
    },
}

pub enum BinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    Equals,
    NotEquals,
    And,
    Or,
    Modulo,
}

pub const TRAIT_ADD: Id<Trait> = Id::<Trait>::new(0);
pub const TRAIT_SUB: Id<Trait> = Id::<Trait>::new(1);
pub const TRAIT_MUL: Id<Trait> = Id::<Trait>::new(2);
pub const TRAIT_DIV: Id<Trait> = Id::<Trait>::new(3);
pub const TRAIT_EQ: Id<Trait> = Id::<Trait>::new(4);
pub const TRAIT_NEQ: Id<Trait> = Id::<Trait>::new(5);
pub const TRAIT_LT: Id<Trait> = Id::<Trait>::new(6);
pub const TRAIT_GT: Id<Trait> = Id::<Trait>::new(7);
pub const TRAIT_LT_EQ: Id<Trait> = Id::<Trait>::new(8);
pub const TRAIT_GT_EQ: Id<Trait> = Id::<Trait>::new(9);
pub const TRAIT_AND: Id<Trait> = Id::<Trait>::new(10);
pub const TRAIT_OR: Id<Trait> = Id::<Trait>::new(11);
pub const TRAIT_MODULO: Id<Trait> = Id::<Trait>::new(12);

impl BinOp {
    pub fn trait_id(&self) -> Id<Trait> {
        match self {
            BinOp::Add => TRAIT_ADD,
            BinOp::Subtract => TRAIT_SUB,
            BinOp::Multiply => TRAIT_MUL,
            BinOp::Divide => TRAIT_DIV,
            BinOp::LessThan => TRAIT_LT,
            BinOp::GreaterThan => TRAIT_GT,
            BinOp::LessThanOrEqual => TRAIT_LT_EQ,
            BinOp::GreaterThanOrEqual => TRAIT_GT_EQ,
            BinOp::Equals => TRAIT_EQ,
            BinOp::NotEquals => TRAIT_NEQ,
            BinOp::And => TRAIT_AND,
            BinOp::Or => TRAIT_OR,
            BinOp::Modulo => TRAIT_MODULO,
        }
    }
}
