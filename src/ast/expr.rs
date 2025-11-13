use crate::{lexer::Identifier, Phase, Span};

#[derive(Debug)]
pub struct Expression<P: Phase> {
    pub expr: Expr<P>,
    pub span: Span,
    pub ty: P::TypeInfo,
}

#[derive(Debug)]
pub enum Expr<P: Phase> {
    Primary(Primary),
    Unary(Unary<P>),
    Binary(Binary<P>),
    Group(Group<P>),
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

#[derive(Debug)]
pub enum Unary<P: Phase> {
    Negate(Box<Expression<P>>),
}

#[derive(Debug)]
pub enum Binary<P: Phase> {
    Add(Box<Expression<P>>, Box<Expression<P>>),
    Subtract(Box<Expression<P>>, Box<Expression<P>>),
    Multiply(Box<Expression<P>>, Box<Expression<P>>),
    Divide(Box<Expression<P>>, Box<Expression<P>>),
}

#[derive(Debug)]
pub struct Group<P: Phase> {
    pub expr: Box<Expression<P>>,
}
