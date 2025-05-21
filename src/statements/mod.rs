use declaration::DeclarationStatement;

use crate::prelude::*;

pub type Statement = GenericStatement<Expression>;
pub type TypedStatement = GenericStatement<TypedExpression>;

pub mod declaration;

#[derive(Debug, Clone, PartialEq)]
pub struct GenericStatement<Expression> {
    pub value: StatementValue<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatementValue<Expression> {
    Expression(Expression),
    Declaration(DeclarationStatement<Expression>),
}

impl StatementValue<Expression> {
    pub fn with_span(self, span: impl Into<Span>) -> Statement {
        Statement {
            value: self,
            span: span.into(),
        }
    }
}
