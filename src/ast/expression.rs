use span_derive::Span;
use std::{collections::HashMap, fmt::Display};

use super::{Identifier, Statement, TypedStatement, Typing};

#[derive(Debug, Clone, Span)]
pub struct Expression {
    pub value: ExpressionValue<Statement, Expression>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone, Span)]
pub struct TypedExpression {
    pub value: ExpressionValue<TypedStatement, TypedExpression>,
    pub span: miette::SourceSpan,
    pub ty: Typing,
}

#[derive(Debug, Clone)]
pub enum ExpressionValue<Statement, Expression> {
    Primitive(Primitive),
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
        function: Box<Expression>,
        arguments: Vec<Expression>,
    },
    VariableAssignment {
        identifier: Identifier,
        argument: Box<Expression>,
    },
    Lambda {
        parameters: Vec<Parameter>,
        explicit_return_type: Option<Box<Typing>>,
        body: Box<Expression>,
    },
    StructConstructor {
        identifier: Identifier,
        arguments: HashMap<Identifier, Expression>,
    },
    FieldAccess {
        parent_identifier: Identifier,
        identifier: Identifier,
    },
}

impl ExpressionValue<Statement, Expression> {
    pub fn with_span(self, span: miette::SourceSpan) -> Expression {
        Expression { value: self, span }
    }
}

#[derive(Debug, Clone)]
pub enum Primitive {
    Integer(i64),
    Decimal(f64),
    String(Box<str>),
    Identifier(Identifier),
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

#[derive(Debug, Clone, Span, Eq)]
pub struct Parameter {
    pub identifier: Identifier,
    pub span: miette::SourceSpan,
    pub ty: Typing,
}

impl PartialEq for Parameter {
    fn eq(&self, other: &Self) -> bool {
        self.identifier == other.identifier && self.ty == other.ty
    }
}
