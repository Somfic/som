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
pub mod typechecker;
pub mod typing;

pub struct Parser<'de> {
    lexer: Lexer<'de>,
    lookup: Lookup<'de>,
}

impl<'de> Parser<'de> {
    pub fn new(lexer: Lexer<'de>) -> Self {
        Self {
            lexer,
            lookup: Lookup::default(),
        }
    }

    pub fn parse(&mut self) -> Result<Symbol<'de>> {
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
