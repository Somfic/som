use crate::{Id, Ident, Path, TypeExpr};

pub enum Expr {
    /// Placeholder for code that failed to parse.
    Hole,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Box<str>),
    /// `none`: an optional without a value.
    None,
    Var(Path),
    Binary {
        op: BinOp,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
    },
    /// `!expr`
    Not(Id<Expr>),
    /// `&expr` or `&mut expr`
    Borrow {
        mutable: bool,
        expr: Id<Expr>,
    },
    /// `*expr`
    Deref(Id<Expr>),
    /// `out name`, as an argument: declares `name` in the surrounding block, starting
    /// out zeroed, and passes `&mut name` for the function to fill in.
    Out(Ident),
    /// `target = value`
    Assign {
        target: Id<Expr>,
        value: Id<Expr>,
    },
    /// `math::add(1, 2)`
    Call {
        callee: Path,
        args: Vec<Id<Expr>>,
    },
    /// `point.sum(1)`
    MethodCall {
        receiver: Id<Expr>,
        method: Ident,
        args: Vec<Id<Expr>>,
    },
    /// `point.x`
    Field {
        object: Id<Expr>,
        field: Ident,
    },
    /// `Point { x: 1, y: 2 }`
    Construct {
        ty: Path,
        fields: Vec<(Ident, Id<Expr>)>,
    },
    /// `then if condition else otherwise`
    Conditional {
        condition: Id<Expr>,
        then: Id<Expr>,
        otherwise: Id<Expr>,
    },
    /// `optional else fallback`: the optional's value, or `fallback` if it has none.
    Fallback {
        optional: Id<Expr>,
        fallback: Id<Expr>,
    },
    /// `{ stmts; value }`
    Block {
        stmts: Vec<Id<Stmt>>,
        value: Option<Id<Expr>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    Equals,
    NotEquals,
    And,
    Or,
}

pub enum Stmt {
    Let {
        name: Ident,
        mutable: bool,
        ty: Option<Id<TypeExpr>>,
        value: Id<Expr>,
    },
    Expr(Id<Expr>),
    Loop {
        body: Vec<Id<Stmt>>,
    },
    While {
        condition: Id<Expr>,
        body: Vec<Id<Stmt>>,
    },
    /// `break;`: leaves the innermost `loop` or `while`.
    Break,
    /// `if condition { then } else { otherwise }`. The condition is a `bool`, or an optional
    /// that's true when it holds a value: `if x` is `if x != none`.
    If {
        condition: Id<Expr>,
        then: Vec<Id<Stmt>>,
        otherwise: Option<Vec<Id<Stmt>>>,
    },
}
