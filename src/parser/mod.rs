use crate::lexer::Lexer;
use ast::Statement;
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

    pub fn parse(&mut self) -> Result<Vec<Statement<'ast>>> {
        let mut statements = vec![];

        while self.lexer.peek().is_some() {
            statements.push(statement::parse(self, false)?);
        }

        Ok(statements)
    }
}
