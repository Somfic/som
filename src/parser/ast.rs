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
    Assignment(Cow<'de, str>, Expression<'de>),
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
    Block {
        statements: Vec<Statement<'de>>,
        return_value: Box<Expression<'de>>,
    },
    If {
        condition: Box<Expression<'de>>,
        truthy: Box<Expression<'de>>,
        falsy: Option<Box<Expression<'de>>>,
    },
}

#[derive(Debug)]
pub enum Primitive<'de> {
    Integer(i64),
    Decimal(f64),
    String(Cow<'de, str>),
    Identifier(Cow<'de, str>),
    Character(char),
    Boolean(bool),
    Unit,
}

#[derive(Debug)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equality,
    Inequality,
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
