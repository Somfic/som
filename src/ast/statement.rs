use super::{Expression, TypedExpression};

pub type TypedStatement<'ast> = GenericStatement<TypedExpression<'ast>>;
pub type Statement<'ast> = GenericStatement<Expression<'ast>>;

#[derive(Debug, Clone)]
pub struct GenericStatement<Expression> {
    pub value: StatementValue<Expression>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StatementValue<Expression> {
    Block(Vec<GenericStatement<Expression>>),
    Expression(Expression),
}
