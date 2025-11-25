use std::cell::Cell;

use crate::lexer::{Lexer, Token, TokenKind};
use crate::{ParserError, Phase, Result, Source, Span};

mod expression;
mod file;
pub mod lookup;
mod program;
mod statement;
mod typing;
pub use program::*;

use lookup::Lookup;

#[derive(Debug)]
pub struct Untyped;

impl Phase for Untyped {
    type TypeInfo = ();
}

pub trait Parse: Sized {
    type Params;

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self>;
}

pub struct Parser {
    pub(crate) lexer: Lexer,
    pub(crate) lookup: Lookup,
    next_function: Cell<usize>,
}

impl Parser {
    pub fn new(source: Source) -> Self {
        Self {
            lexer: Lexer::new(source),
            lookup: Lookup::default(),
            next_function: Cell::new(0),
        }
    }

    pub fn with_function_counter(source: Source, next_function: &Cell<usize>) -> Self {
        // Create parser with a copy of the current counter value
        let start_id = next_function.get();
        Self {
            lexer: Lexer::new(source),
            lookup: Lookup::default(),
            next_function: Cell::new(start_id),
        }
    }

    pub fn next_function_id(&self) -> usize {
        let id = self.next_function.get();
        self.next_function.set(id + 1);
        id
    }

    pub fn get_next_function_id(&self) -> usize {
        self.next_function.get()
    }

    pub fn parse<T: Parse>(&mut self) -> Result<T>
    where
        T::Params: Default,
    {
        self.parse_with(Default::default())
    }

    pub fn parse_with<T: Parse>(&mut self, params: T::Params) -> Result<T> {
        T::parse(self, params)
    }

    pub(crate) fn try_parse<T: Parse>(&mut self) -> Option<T>
    where
        T::Params: Default,
    {
        self.try_parse_with(Default::default())
    }

    pub(crate) fn try_parse_with<T: Parse>(&mut self, params: T::Params) -> Option<T> {
        let checkpoint = self.lexer.cursor.clone();
        match self.parse_with(params) {
            Ok(value) => Some(value),
            Err(_) => {
                self.lexer.cursor = checkpoint;
                None
            }
        }
    }

    pub(crate) fn expect(
        &mut self,
        token: TokenKind,
        expect: impl Into<String>,
        error: ParserError,
    ) -> Result<Token> {
        let next = self.lexer.peek();

        if let Some(next) = next {
            if next.kind == token {
                return self.lexer.next().unwrap();
            }

            return error
                .to_diagnostic()
                .with_label(next.span.clone().label(format!(
                    "expected {} here for {}",
                    token,
                    expect.into()
                )))
                .to_err();
        }

        ParserError::UnexpectedEndOfInput
            .to_diagnostic()
            .with_label(
                self.lexer
                    .cursor
                    .label(format!("expected {} here", expect.into())),
            )
            .to_err()
    }

    pub(crate) fn peek(&mut self) -> Option<&Token> {
        self.lexer.peek()
    }

    pub(crate) fn peek_expect(&mut self, expect: impl Into<String>) -> Result<&Token> {
        let cursor = self.lexer.cursor.clone();

        match self.lexer.peek() {
            Some(token) => Ok(token),
            None => ParserError::UnexpectedEndOfInput
                .to_diagnostic()
                .with_label(cursor.label(format!("expected {} here", expect.into())))
                .to_err(),
        }
    }

    pub(crate) fn next(&mut self) -> Result<Token> {
        self.lexer
            .next()
            .ok_or(ParserError::UnexpectedEndOfInput.to_diagnostic())?
    }

    pub(crate) fn parse_with_span<T: Parse>(&mut self) -> Result<(T, Span)>
    where
        T::Params: Default,
    {
        self.parse_with_span_with(Default::default())
    }

    pub(crate) fn parse_with_span_with<T: Parse>(
        &mut self,
        params: T::Params,
    ) -> Result<(T, Span)> {
        let start = self.lexer.cursor.clone();
        let source_name = self.lexer.source.identifier();
        let source_content = self.lexer.source_content.clone();

        let inner = T::parse(self, params)?;

        let end = self.lexer.cursor.clone();

        let span = start - end;

        Ok((inner, span))
    }
}
