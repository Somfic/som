use crate::lexer::{Lexer, Token, TokenKind};
use crate::{Error, Phase, Result, Source, Span};

mod expr;

#[derive(Debug)]
pub struct ParsePhase;

impl Phase for ParsePhase {
    type TypeInfo = (); // No type info during parsing
}

pub trait Parse: Sized {
    type Params;

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self>;
}

pub struct Parser {
    pub(crate) lexer: Lexer,
}

impl Parser {
    pub fn new(source: Source) -> Self {
        Self {
            lexer: Lexer::new(source),
        }
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

    pub(crate) fn expect(&mut self, token: TokenKind) -> Result<Token> {
        let next = self.lexer.peek();
        if let Some(next) = next {
            if next.kind == token {
                return self.lexer.next().unwrap();
            }

            return Error::ParserError(format!("expected {}, found {}", token, next.kind))
                .to_diagnostic()
                .to_err();
        }

        Err(Error::ParserError("unexpected end of input".into()).into())
    }

    pub(crate) fn peek(&mut self) -> Option<&Token> {
        self.lexer.peek()
    }

    pub(crate) fn peek_expect(&mut self) -> Result<&Token> {
        match self.lexer.peek() {
            Some(token) => Ok(token),
            None => Error::ParserError("unexpected end of file".into())
                .to_diagnostic()
                .to_err(),
        }
    }

    pub(crate) fn next(&mut self) -> Result<Token> {
        self.lexer
            .next()
            .ok_or_else(|| Error::ParserError("unexpected end of input".into()))?
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

pub(crate) fn infix_binding_power(kind: &TokenKind) -> Option<(u8, u8)> {
    Some(match kind {
        TokenKind::Plus | TokenKind::Minus => (9, 10),
        TokenKind::Star | TokenKind::Slash => (11, 12),
        _ => return None,
    })
}
