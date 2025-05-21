use crate::prelude::*;
use binary::BinaryExpression;
use group::GroupExpression;
use primary::PrimaryExpression;

pub mod binary;
pub mod group;
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
    pub value: TypedExpressionValue,
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

pub type ExpressionValue = GenericExpressionValue<Expression>;
pub type TypedExpressionValue = GenericExpressionValue<TypedExpression>;

#[derive(Debug, Clone, PartialEq)]
pub enum GenericExpressionValue<Expression> {
    Primary(PrimaryExpression),
    Binary(BinaryExpression<Expression>),
    Group(GroupExpression<Expression>),
}
