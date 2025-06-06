use crate::prelude::*;
use binary::BinaryExpression;
use block::BlockExpression;
use call::CallExpression;
use function::FunctionExpression;
use group::GroupExpression;
use primary::PrimaryExpression;

pub mod binary;
pub mod block;
pub mod call;
pub mod function;
pub mod group;
pub mod identifier;
pub mod primary;

#[derive(Debug, Clone)]
pub struct Expression {
    pub value: ExpressionValue,
    pub span: Span,
}

impl Expression {
    pub fn with_value_type(&self, value: TypedExpressionValue, type_: Type) -> TypedExpression {
        TypedExpression {
            value,
            span: self.span,
            type_,
        }
    }

    pub fn with_span(self, span: impl Into<Span>) -> Self {
        Self {
            value: self.value.clone(),
            span: span.into(),
        }
    }
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

#[derive(Clone, Debug)]
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

impl From<TypedExpression> for miette::SourceSpan {
    fn from(typed_expression: TypedExpression) -> Self {
        typed_expression.span.into()
    }
}

impl From<&TypedExpression> for miette::SourceSpan {
    fn from(expression: &TypedExpression) -> Self {
        expression.span.into()
    }
}


pub type ExpressionValue = GenericExpressionValue<Expression>;
pub type TypedExpressionValue = GenericExpressionValue<TypedExpression>;

#[derive(Debug, Clone)]
pub enum GenericExpressionValue<Expression> {
    Primary(PrimaryExpression),
    Binary(BinaryExpression<Expression>),
    Group(GroupExpression<Expression>),
    Block(BlockExpression<Expression>),
    Identifier(Identifier),
    Function(FunctionExpression<Expression>),
    Call(CallExpression<Expression>),
}

impl GenericExpressionValue<Expression> {
    pub fn with_span(self, span: impl Into<Span>) -> Expression {
        Expression {
            value: self,
            span: span.into(),
        }
    }
}
