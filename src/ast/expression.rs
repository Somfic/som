use std::{borrow::Cow, fmt::Display};

use super::{Statement, Type};

#[derive(Debug, Clone)]
pub struct Expression<'ast> {
    pub value: ExpressionValue<'ast, Expression<'ast>>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub struct TypedExpression<'ast> {
    pub value: ExpressionValue<'ast, TypedExpression<'ast>>,
    pub span: miette::SourceSpan,
    pub ty: Type<'ast>,
}

#[derive(Debug, Clone)]
pub enum ExpressionValue<'ast, Expression> {
    Primitive(Primitive<'ast>),
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Group(Box<Expression>),
    Conditional {
        condition: Box<Expression>,
        truthy: Box<Expression>,
        falsy: Box<Expression>,
    },
    // Block {
    //     statements: Vec<Statement<'ast>>,
    //     return_value: Box<Expression>,
    // },
}

#[derive(Debug, Clone)]
pub enum Primitive<'ast> {
    Integer(i64),
    Decimal(f64),
    String(Cow<'ast, str>),
    Identifier(Cow<'ast, str>),
    Character(char),
    Boolean(bool),
    Unit,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub enum UnaryOperator {
    Negate,
    Negative,
}

impl Display for BinaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryOperator::Add => write!(f, "an addition"),
            BinaryOperator::Subtract => write!(f, "a subtraction"),
            BinaryOperator::Multiply => write!(f, "a multiplication"),
            BinaryOperator::Divide => write!(f, "a division"),
            BinaryOperator::Modulo => write!(f, "a modulo"),
            BinaryOperator::Equality => write!(f, "an equality"),
            BinaryOperator::Inequality => write!(f, "an inequality"),
            BinaryOperator::LessThan => write!(f, "a less than"),
            BinaryOperator::LessThanOrEqual => write!(f, "a less than or equal"),
            BinaryOperator::GreaterThan => write!(f, "a greater than"),
            BinaryOperator::GreaterThanOrEqual => write!(f, "a greater than or equal"),
            BinaryOperator::And => write!(f, "an and"),
            BinaryOperator::Or => write!(f, "an or"),
        }
    }
}
