use crate::{
    ast::{Expression, Type},
    lexer::Identifier,
    Phase, Span,
};
use std::fmt::Display;

#[derive(Debug)]
pub enum Statement<P: Phase> {
    Expression(Expression<P>),
    Scope(Scope<P>),
    Declaration(Declaration<P>),
    TypeDefinition(TypeDefinition),
}

impl<P: Phase> Statement<P> {
    pub fn span(&self) -> &Span {
        match self {
            Statement::Expression(e) => &e.span(),
            Statement::Scope(s) => &s.span,
            Statement::Declaration(d) => &d.span,
            Statement::TypeDefinition(t) => &t.span,
        }
    }
}

impl<P: Phase> Display for Statement<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Statement::Expression(expression) => write!(f, "{}", expression),
            Statement::Scope(scope) => write!(f, "a scope"),
            Statement::Declaration(declaration) => write!(f, "a declaration"),
            Statement::TypeDefinition(type_definition) => {
                write!(f, "a type definition")
            }
        }
    }
}

#[derive(Debug)]
pub struct Scope<P: Phase> {
    pub statements: Vec<Statement<P>>,
    pub span: Span,
}

#[derive(Debug)]
pub struct Declaration<P: Phase> {
    pub name: Identifier,
    pub value: Box<Expression<P>>,
    pub span: Span,
}

#[derive(Debug)]
pub struct TypeDefinition {
    pub name: Identifier,
    pub ty: Type,
    pub span: Span,
}
