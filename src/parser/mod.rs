use crate::lexer::Lexer;
use ast::{Statement, Symbol};
use lookup::Lookup;
use miette::Result;

pub mod ast;
pub mod expression;
pub mod lookup;
pub mod statement;

pub struct Parser<'de> {
    lexer: Lexer<'de>,
    lookup: Lookup<'de>,
}

impl<'de> Parser<'de> {
    pub fn new(input: &'de str) -> Self {
        Parser {
            lexer: Lexer::new(input),
            lookup: Lookup::default(),
        }
    }

    pub fn parse(&mut self) -> Result<Symbol<'de>> {
        let mut statements = vec![];

        while self.lexer.peek().is_some() {
            statements.push(statement::parse(self, false)?);
        }

        Ok(Symbol::Statement(Statement::Block(statements)))
    }
}
