use std::fmt::{write, Display};

use crate::{ast::Statement, lexer::Identifier, Phase, Span};

#[derive(Debug)]
pub struct Expression<P: Phase> {
    pub expr: Expr<P>,
    pub span: Span,
    pub ty: P::TypeInfo,
}

impl<P: Phase> Display for Expression<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.expr)
    }
}

#[derive(Debug)]
pub enum Expr<P: Phase> {
    Primary(Primary),
    Unary(Unary<P>),
    Binary(Binary<P>),
    Group(Group<P>),
    Block(Block<P>),
}

impl<P: Phase> Display for Expr<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Primary(primary) => write!(f, "{}", primary),
            Expr::Unary(unary) => write!(f, "{}", unary),
            Expr::Binary(binary) => write!(f, "{}", binary),
            Expr::Group(group) => write!(f, "{}", group.expr),
            Expr::Block(block) => write!(f, "a block"),
        }
    }
}

#[derive(Debug)]
pub enum Primary {
    Boolean(bool),
    I32(i32),
    I64(i64),
    Decimal(f64),
    String(Box<str>),
    Character(char),
    Identifier(Identifier),
}

impl Display for Primary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Primary::Boolean(b) => write!(f, "a boolean"),
            Primary::I32(i) => write!(f, "a number"),
            Primary::I64(i) => write!(f, "a number"),
            Primary::Decimal(d) => write!(f, "a number"),
            Primary::String(s) => write!(f, "a string"),
            Primary::Character(c) => write!(f, "a character"),
            Primary::Identifier(id) => write!(f, "an identifier"),
        }
    }
}

#[derive(Debug)]
pub enum Unary<P: Phase> {
    Negate(Box<Expression<P>>),
}

impl<P: Phase> Display for Unary<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unary::Negate(expression) => write!(f, "a negation"),
        }
    }
}

#[derive(Debug)]
pub struct Binary<P: Phase> {
    pub lhs: Box<Expression<P>>,
    pub rhs: Box<Expression<P>>,
    pub op: BinaryOperation,
}

impl<P: Phase> Display for Binary<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.op {
            BinaryOperation::Add => write!(f, "an addition"),
            BinaryOperation::Subtract => write!(f, "a subtraction"),
            BinaryOperation::Multiply => write!(f, "a multiplication"),
            BinaryOperation::Divide => write!(f, "a division"),
        }
    }
}

#[derive(Debug)]
pub enum BinaryOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Debug)]
pub struct Group<P: Phase> {
    pub expr: Box<Expression<P>>,
}

#[derive(Debug)]
pub struct Block<P: Phase> {
    pub statements: Vec<Statement<P>>,
    pub expression: Option<Box<Expression<P>>>,
}
