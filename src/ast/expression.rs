use std::{borrow::Cow, fmt::Display};

use super::{ParameterDeclaration, Statement, Type};

impl Display for ExpressionValue<'_, Expression<'_>> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpressionValue::Primitive(primitive) => write!(f, "{}", primitive),
            ExpressionValue::Binary {
                operator,
                left: _,
                right: _,
            } => write!(f, "{} expression", operator),
            ExpressionValue::Unary {
                operator,
                operand: _,
            } => write!(f, "{} expression", operator),
            ExpressionValue::Group(_expression) => write!(f, "grouped expression"),
            ExpressionValue::Block {
                statements: _,
                return_value: _,
            } => write!(f, "block expression"),
            ExpressionValue::Conditional {
                condition: _,
                truthy: _,
                falsy: _,
            } => write!(f, "conditional expression"),
            ExpressionValue::Call {
                callee,
                arguments: _,
            } => write!(f, "calling {}", callee.value),
            ExpressionValue::Lambda(_) => write!(f, "lambda expression"),
        }
    }
}

impl Display for Primitive<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Primitive::Integer(value) => write!(f, "{}", value),
            Primitive::Decimal(value) => write!(f, "{}", value),
            Primitive::String(value) => write!(f, "{}", value),
            Primitive::Identifier(value) => write!(f, "{}", value),
            Primitive::Character(value) => write!(f, "{}", value),
            Primitive::Boolean(value) => write!(f, "{}", value),
            Primitive::Unit => write!(f, "nothing"),
        }
    }
}

impl Display for Expression<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Display for BinaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryOperator::Add => write!(f, "addition"),
            BinaryOperator::Subtract => write!(f, "subtraction"),
            BinaryOperator::Multiply => write!(f, "multiplication"),
            BinaryOperator::Divide => write!(f, "division"),
            BinaryOperator::Modulo => write!(f, "modulo"),
            BinaryOperator::Equality => write!(f, "equality"),
            BinaryOperator::Inequality => write!(f, "inequality"),
            BinaryOperator::LessThan => write!(f, "less than comparison"),
            BinaryOperator::LessThanOrEqual => write!(f, "less than or equal comparison"),
            BinaryOperator::GreaterThan => write!(f, "greater than comparison"),
            BinaryOperator::GreaterThanOrEqual => write!(f, "greater than or equal comparison"),
            BinaryOperator::And => write!(f, "and"),
            BinaryOperator::Or => write!(f, "or"),
        }
    }
}

impl Display for UnaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnaryOperator::Negate => write!(f, "negation"),
            UnaryOperator::Negative => write!(f, "negative"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Expression<'ast> {
    pub value: ExpressionValue<'ast, Expression<'ast>>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub struct TypedExpression<'ast> {
    pub value: ExpressionValue<'ast, Expression<'ast>>,
    pub span: miette::SourceSpan,
    pub ty: Type<'ast>,
}

impl Expression<'_> {
    pub fn label(&self, label: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, label)
    }
}

impl<'ast> Expression<'ast> {
    pub fn to_typed(self, ty: Type<'ast>) -> TypedExpression<'ast> {
        TypedExpression {
            value: self.value,
            span: self.span,
            ty,
        }
    }
}

impl TypedExpression<'_> {
    pub fn label(&self, label: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, label)
    }
}

#[derive(Debug, Clone)]
pub enum ExpressionValue<'ast, Expression> {
    Primitive(Primitive<'ast>),
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
    Group(Box<Expression>),
    Block {
        statements: Vec<Statement<'ast, Expression>>,
        return_value: Box<Expression>,
    },
    Conditional {
        condition: Box<Expression>,
        truthy: Box<Expression>,
        falsy: Box<Expression>,
    },
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
    },
    Lambda(Lambda<'ast>),
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
    pub fn is_comparison(&self) -> bool {
        matches!(
            self,
            BinaryOperator::Equality
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

#[derive(Debug, Clone)]
pub struct Lambda<'ast> {
    pub parameters: Vec<ParameterDeclaration<'ast>>,
    pub body: Box<Expression<'ast>>,
}
