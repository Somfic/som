use super::{Expression, Identifier, Parameter, TypedExpression, Typing};

use span_derive::Span;
use std::collections::HashMap;

pub type TypedStatement = GenericStatement<TypedExpression>;
pub type Statement = GenericStatement<Expression>;

#[derive(Debug, Clone, Span)]
pub struct GenericStatement<Expression> {
    pub value: StatementValue<Expression>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StatementValue<Expression> {
    Block(Vec<GenericStatement<Expression>>),
    Expression(Expression),
    Condition(Expression, Box<GenericStatement<Expression>>),
    WhileLoop(Expression, Box<GenericStatement<Expression>>),
    Declaration {
        identifier: Identifier,
        explicit_type: Option<Typing>,
        value: Expression,
    },
}

impl StatementValue<Expression> {
    pub fn with_span(self, span: miette::SourceSpan) -> Statement {
        Statement { value: self, span }
    }
}

impl GenericStatement<Expression> {
    pub fn expression(span: miette::SourceSpan, value: Expression) -> GenericStatement<Expression> {
        Self {
            value: StatementValue::Expression(value),
            span,
        }
    }
}

#[derive(Debug, Clone, Span, Eq)]
pub struct LambdaSignature {
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter>,
    pub explicit_return_type: Option<Box<Typing>>,
}

impl PartialEq for LambdaSignature {
    fn eq(&self, other: &Self) -> bool {
        self.parameters == other.parameters
            && self.explicit_return_type == other.explicit_return_type
    }
}

#[derive(Debug, Clone, Span)]
pub struct IntrinsicSignature {
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter>,
    pub return_type: Typing,
}
