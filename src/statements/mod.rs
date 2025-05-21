use declaration::DeclarationStatement;

use crate::prelude::*;

pub type Statement = GenericStatement<Expression>;
pub type TypedStatement = GenericStatement<TypedExpression>;

pub mod declaration;

#[derive(Debug, Clone)]
pub struct GenericStatement<Expression> {
    pub value: StatementValue<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum StatementValue<Expression> {
    Expression(Expression),
    Declaration(DeclarationStatement<Expression>),
}
