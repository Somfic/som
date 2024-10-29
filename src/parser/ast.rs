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
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression<'de>>,
    },
    Group(Box<Expression<'de>>),
}

#[derive(Debug)]
pub enum Primitive<'de> {
    Integer(i64),
    Decimal(f64),
    String(Cow<'de, str>),
    Boolean(bool),
}

#[derive(Debug)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    And,
    Or,
}

#[derive(Debug)]
pub enum UnaryOperator {
    Negate,
    Negative,
}
