pub use crate::prelude::*;

pub struct TypedExpression {
    pub value: ExpressionValue,
    pub span: SourceSpan,
    pub type_: Type,
}

pub type Statement = GenericStatement<Expression>;
pub type TypedStatement = GenericStatement<TypedExpression>;
pub struct GenericStatement<Expression> {
    pub value: StatementValue<Expression>,
    pub span: SourceSpan,
}
