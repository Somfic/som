use std::borrow::Cow;

use crate::{
    ast::{Expression, Module},
    lexer::Lexer,
};
use lookup::Lookup;
use miette::Result;

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

    pub fn parse(&mut self) -> Result<Module<'ast, Expression<'ast>>> {
        let mut module = Module {
            name: Cow::Borrowed("main"),
            definitions: vec![],
        };

        while self.lexer.peek().is_some() {
            module.definitions.push(statement::parse(self, false)?);
        }

        Ok(module)
    }
}
