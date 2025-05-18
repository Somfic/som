use crate::expressions::{Expression, TypedExpression};

pub type Statement = GenericStatement<Expression>;
pub type TypedStatement = GenericStatement<TypedExpression>;

#[derive(Debug, Clone)]
pub struct GenericStatement<Expression> {
    pub value: StatementValue<Expression>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StatementValue<Expression> {
    Expression(Expression),
}
