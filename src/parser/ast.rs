use std::borrow::Cow;

#[derive(Debug)]
pub enum Symbol<'de> {
    Statement(Statement<'de>),
    Expression(Expression<'de>),
}

#[derive(Debug)]
pub enum Statement<'de> {
    Block(Vec<Statement<'de>>),
    Expression(Expression<'de>),
}

#[derive(Debug)]
pub enum Expression<'de> {
    Primitive(Primitive<'de>),
    Binary {
        operator: BinaryOperator,
        left: Box<Expression<'de>>,
        right: Box<Expression<'de>>,
    },
}

#[derive(Debug)]
pub enum Primitive<'de> {
    Integer(i64),
    Float(f64),
    String(Cow<'de, str>),
    Boolean(bool),
}

#[derive(Debug)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}
