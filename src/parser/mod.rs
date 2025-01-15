use crate::lexer::Lexer;
use ast::{
    untyped::{Statement, StatementValue, Symbol},
    Spannable,
};
use lookup::Lookup;
use miette::Result;

pub mod ast;
pub mod expression;
pub mod lookup;
pub mod statement;
pub mod typing;

pub struct Parser<'ast> {
    lexer: Lexer<'ast>,
    lookup: Lookup<'ast>,
}

impl<'ast> Parser<'ast> {
    pub fn new(lexer: Lexer<'ast>) -> Self {
        Self {
            lexer,
            lookup: Lookup::default(),
        }
    }

    pub fn parse(&mut self) -> Result<Symbol<'ast>> {
        let mut statements = vec![];

        while self.lexer.peek().is_some() {
            statements.push(statement::parse(self, false)?);
        }

        Ok(Symbol::Statement(Statement::at_multiple(
            statements.iter().map(|s| s.span).collect(),
            StatementValue::Block(statements),
        )))
    }
}
