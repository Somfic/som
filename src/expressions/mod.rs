use crate::prelude::*;
use binary::BinaryExpression;
use primary::PrimaryExpression;

pub mod binary;

pub mod primary;

#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    pub value: ExpressionValue,
    pub span: Span,
}

impl From<Expression> for Span {
    fn from(expression: Expression) -> Self {
        expression.span
    }
}

impl From<&Expression> for Span {
    fn from(expression: &Expression) -> Self {
        expression.span
    }
}

pub struct TypedExpression {
    pub value: ExpressionValue,
    pub span: Span,
    pub type_: Type,
}

impl From<TypedExpression> for Span {
    fn from(typed_expression: TypedExpression) -> Self {
        typed_expression.span
    }
}

impl From<&TypedExpression> for Span {
    fn from(expression: &TypedExpression) -> Self {
        expression.span
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionValue {
    Primary(PrimaryExpression),
    Binary(BinaryExpression),
}
