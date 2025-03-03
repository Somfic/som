use std::{borrow::Cow, fmt::Display};

use miette::SourceSpan;

use super::{Statement, TypedStatement, Typing};

#[derive(Debug, Clone)]
pub struct Expression<'ast> {
    pub value: ExpressionValue<'ast, Statement<'ast>, Expression<'ast>>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub struct TypedExpression<'ast> {
    pub value: ExpressionValue<'ast, TypedStatement<'ast>, TypedExpression<'ast>>,
    pub span: miette::SourceSpan,
    pub ty: Typing<'ast>,
}

#[derive(Debug, Clone)]
pub enum ExpressionValue<'ast, Statement, Expression> {
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
    Block {
        statements: Vec<Statement>,
        result: Box<Expression>,
    },
    FunctionCall {
        function_name: Cow<'ast, str>,
        arguments: Vec<Expression>,
    },
    Assignment {
        name: Cow<'ast, str>,
        value: Box<Expression>,
    },
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

impl BinaryOperator {
    pub fn is_logical(&self) -> bool {
        matches!(
            self,
            BinaryOperator::And
                | BinaryOperator::Or
                | BinaryOperator::Equality
                | BinaryOperator::Inequality
                | BinaryOperator::LessThan
                | BinaryOperator::LessThanOrEqual
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterThanOrEqual
        )
    }
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

impl Expression<'_> {
    pub fn label(&self, text: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, text.into())
    }

    pub fn span(mut self, span: SourceSpan) -> Self {
        self.span = span;
        self
    }
}
