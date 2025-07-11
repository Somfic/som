use crate::prelude::*;

pub type Statement = GenericStatement<Expression>;
pub type TypedStatement = GenericStatement<TypedExpression>;

pub mod extern_declaration;
pub mod type_declaration;
pub mod variable_declaration;

#[derive(Debug, Clone, PartialEq)]
pub struct GenericStatement<Expression> {
    pub value: StatementValue<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatementValue<Expression> {
    Expression(Expression),
    VariableDeclaration(VariableDeclarationStatement<Expression>),
    ExternDeclaration(ExternDeclarationStatement),
    TypeDeclaration(TypeDeclarationStatement),
}

impl StatementValue<Expression> {
    pub fn with_span(self, span: impl Into<Span>) -> Statement {
        Statement {
            value: self,
            span: span.into(),
        }
    }
}
