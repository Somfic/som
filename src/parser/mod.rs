use crate::lexer::{Lexer, Token, TokenKind};
use ast::{Statement, Symbol};
use lookup::Lookup;
use miette::{Context, Error, Result};
use std::{borrow::Cow, collections::HashMap, os::macos};

pub mod ast;
pub mod expression;
pub mod lookup;

pub struct Parser<'de> {
    source: &'de str,
    lexer: Lexer<'de>,
    lookup: Lookup<'de>,
}

impl<'de> Parser<'de> {
    pub fn new(input: &'de str) -> Self {
        Parser {
            source: input,
            lexer: Lexer::new(input),
            lookup: Lookup::default(),
        }
    }

    pub fn parse(&mut self) -> Result<Symbol<'de>> {
        expression::parse(self)?;
        todo!()
    }
}
