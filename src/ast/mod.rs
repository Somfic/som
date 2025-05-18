pub use crate::prelude::*;

pub type Statement = GenericStatement<Expression>;
pub type TypedStatement = GenericStatement<TypedExpression>;
pub struct GenericStatement<Expression> {
    pub value: StatementValue<Expression>,
    pub span: SourceSpan,
}
