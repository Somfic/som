use logos::Logos;
use som_common::*;

use crate::{Token, TokenKind};

pub fn lex(source: Id<Source>, content: &str) -> Vec<Token> {
    debug!(source = ?source, "Lexing");

    let mut lexer = TokenKind::lexer(content);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        let kind = result.unwrap_or(TokenKind::Error);
        let span_range = lexer.span();
        let text = lexer.slice();

        if kind == TokenKind::Whitespace {
            continue;
        }

        tokens.push(Token {
            kind,
            text: text.into(),
            span: Span::from_range(source, span_range),
        });
    }

    // EOF
    let eof_pos = content.len();
    tokens.push(Token {
        kind: TokenKind::Eof,
        text: "".into(),
        span: Span::from_range(source, eof_pos..eof_pos),
    });

    debug!(source = ?source, token_count = tokens.len(), "Lexing complete");

    tokens
}
