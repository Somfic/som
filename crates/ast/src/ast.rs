use std::ops::Index;

use som_common::{Arena, Id, Span, expand_enum};

#[derive(Debug, Default)]
pub struct Ast {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    pub root: Vec<Id<Stmt>>,
}

impl Ast {
    pub fn new() -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            root: Vec::new(),
        }
    }

    pub fn add_expr(&mut self, expr: Expr) -> Id<Expr> {
        self.exprs.alloc(expr)
    }

    pub fn add_stmt(&mut self, stmt: Stmt) -> Id<Stmt> {
        self.stmts.alloc(stmt)
    }
}

impl Index<Id<Expr>> for Ast {
    type Output = Expr;
    fn index(&self, id: Id<Expr>) -> &Expr {
        self.exprs.get(&id)
    }
}

impl Index<Id<Stmt>> for Ast {
    type Output = Stmt;
    fn index(&self, id: Id<Stmt>) -> &Stmt {
        self.stmts.get(&id)
    }
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug, Clone, Copy)]
    pub enum Expr {
        Error,
        Int { value: i64 },
        Bool { value: bool },
        Unary { op: UnaryOp, operand: Id<Expr> },
        Binary { lhs: Id<Expr>, op: BinaryOp, rhs: Id<Expr> },
        Condition { condition: Id<Expr>, truthy: Id<Expr>, falsy: Id<Expr> },
    } with { span: Span }
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug)]
    pub enum Stmt {
        Expr { expr: Id<Expr> },
    } with { span: Span }
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Negate,
    Not,
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            UnaryOp::Negate => "-",
            UnaryOp::Not => "!",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equals,
    NotEquals,
    LessThan,
    LessThanOrEquals,
    GreaterThan,
    GreaterThanOrEquals,
    And,
    Or,
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Equals => "==",
            BinaryOp::NotEquals => "!=",
            BinaryOp::LessThan => "<",
            BinaryOp::LessThanOrEquals => "<=",
            BinaryOp::GreaterThan => ">",
            BinaryOp::GreaterThanOrEquals => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        };
        f.write_str(s)
    }
}
