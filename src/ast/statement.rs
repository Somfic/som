use std::fmt::{write, Display};

use crate::{ast::Expression, lexer::Identifier, Phase};

#[derive(Debug)]
pub enum Statement<P: Phase> {
    Expression(Expression<P>),
    Scope(Scope<P>),
    Declaration(Declaration<P>),
}

impl<P: Phase> Display for Statement<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Statement::Expression(expression) => write!(f, "{}", expression),
            Statement::Scope(scope) => write!(f, "a scope"),
            Statement::Declaration(declaration) => write!(f, "a declaration"),
        }
    }
}

#[derive(Debug)]
pub struct Scope<P: Phase> {
    pub statements: Vec<Statement<P>>,
}

#[derive(Debug)]
pub struct Declaration<P: Phase> {
    pub name: Identifier,
    pub value: Box<Expression<P>>,
}
