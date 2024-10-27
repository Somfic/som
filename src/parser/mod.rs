use crate::lexer::{Lexer, Token, TokenKind};
use ast::{Statement, Symbol};
use miette::{Context, Error, Result};
use std::{borrow::Cow, collections::HashMap, os::macos};

pub mod ast;
pub mod lookup;

pub struct Parser<'de> {
    source: &'de str,
    lexer: Lexer<'de>,
}

impl<'de> Parser<'de> {
    pub fn new(input: &'de str) -> Self {
        Parser {
            source: input,
            lexer: Lexer::new(input),
        }
    }

    pub fn parse(&mut self) -> Result<Symbol<'de>> {
        self.lexer.expect(TokenKind::String, "expected backtick")?;

        Ok(Symbol::Statement(Statement::Block(vec![])))
    }
}
