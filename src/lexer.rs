use miette::SourceOffset;

use crate::prelude::*;

pub struct Lexer<'source> {
    source: &'source str,
    remainer: &'source str,
    byte_offset: usize,
    peeked: Option<Result<Token>>,
}

impl<'source> Lexer<'source> {
    pub fn new(source: &'source str) -> Self {
        Self {
            source,
            remainer: source,
            byte_offset: 0,
            peeked: None,
        }
    }

    pub fn next_token(&mut self) -> Result<Token> {
        Err(vec![Error::Lexer(LexerError::UnexpectedCharacter {
            token: Token {
                kind: TokenKind::Integer,
                value: TokenValue::Integer(0),
                span: SourceSpan::new(1.into(), 1),
            },
            help: Some("This is a help message".to_string()),
        })])
    }
}

#[derive(Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub value: TokenValue,
    pub span: SourceSpan,
}

impl From<&Token> for SourceSpan {
    fn from(token: &Token) -> Self {
        token.span
    }
}

#[derive(Debug)]
pub enum TokenKind {
    Integer,
}

#[derive(Debug)]
pub enum TokenValue {
    Integer(i64),
}
