use crate::span::{Source, Span};
use logos::Logos;
use std::sync::Arc;

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // Keywords
    #[token("fn")]
    Fn,
    #[token("extern")]
    Extern,
    #[token("let")]
    Let,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("mut")]
    Mut,

    // Built-in types
    #[token("i8")]
    I8,
    #[token("i16")]
    I16,
    #[token("i32")]
    I32,
    #[token("i64")]
    I64,
    #[token("i128")]
    I128,
    #[token("isize")]
    ISize,
    #[token("u8")]
    U8,
    #[token("u16")]
    U16,
    #[token("u32")]
    U32,
    #[token("u64")]
    U64,
    #[token("u128")]
    U128,
    #[token("usize")]
    USize,
    #[token("f32")]
    F32,
    #[token("f64")]
    F64,
    #[token("bool")]
    Bool,
    #[token("char")]
    Char,
    #[token("str")]
    Str,

    // Literals and identifiers
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,
    #[regex(r"[0-9]+")]
    Int,
    #[regex(r#""([^"\\]|\\.)*""#)]
    Text,
    #[regex(r"true")]
    True,
    #[regex(r"false")]
    False,

    // Operators
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("=")]
    Equals,
    #[token("==")]
    DoubleEquals,
    #[token("!=")]
    NotEquals,
    #[token("<")]
    LessThan,
    #[token(">")]
    GreaterThan,
    #[token("<=")]
    LessThanOrEqual,
    #[token(">=")]
    GreaterThanOnEqual,

    // Delimiters
    #[token("(")]
    OpenParen,
    #[token(")")]
    CloseParen,
    #[token("{")]
    OpenBrace,
    #[token("}")]
    CloseBrace,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token(":")]
    Colon,
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("&")]
    Ampersand,

    #[token("'")]
    SingleQuote,
    #[token("\"")]
    DoubleQuote,

    // Whitespace and comments (skipped during parsing)
    #[regex(r"[ \t\r\n]+")]
    Whitespace,
    #[regex(r"//[^\n]*", allow_greedy = true)]
    Comment,

    // Special
    Error,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token<'src> {
    pub kind: TokenKind,
    pub text: &'src str,
    pub span: Span,
}

pub fn lex(source: Arc<Source>) -> Vec<Token<'static>> {
    let input = source.content();
    let mut lexer = TokenKind::lexer(input);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        let kind = result.unwrap_or(TokenKind::Error);
        let span_range = lexer.span();
        let text = lexer.slice();
        tokens.push(Token {
            kind,
            text: Box::leak(text.to_string().into_boxed_str()),
            span: Span::from_range(span_range, source.clone()),
        });
    }

    // Add EOF token
    let eof_pos = input.len();
    tokens.push(Token {
        kind: TokenKind::Eof,
        text: "",
        span: Span::from_range(eof_pos..eof_pos, source),
    });

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_keywords() {
        let source = Arc::new(Source::from_raw("fn let if else"));
        let tokens = lex(source);
        let non_ws: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .collect();
        assert_eq!(non_ws[0].kind, TokenKind::Fn);
        assert_eq!(non_ws[1].kind, TokenKind::Let);
        assert_eq!(non_ws[2].kind, TokenKind::If);
        assert_eq!(non_ws[3].kind, TokenKind::Else);
    }

    #[test]
    fn test_lex_identifiers() {
        let source = Arc::new(Source::from_raw("foo bar_baz x123"));
        let tokens = lex(source);
        let non_ws: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .collect();
        assert_eq!(non_ws[0].kind, TokenKind::Ident);
        assert_eq!(non_ws[0].text, "foo");
        assert_eq!(non_ws[1].kind, TokenKind::Ident);
        assert_eq!(non_ws[1].text, "bar_baz");
    }

    #[test]
    fn test_lex_integers() {
        let source = Arc::new(Source::from_raw("123 456"));
        let tokens = lex(source);
        let non_ws: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .collect();
        assert_eq!(non_ws[0].kind, TokenKind::Int);
        assert_eq!(non_ws[0].text, "123");
        assert_eq!(non_ws[1].kind, TokenKind::Int);
        assert_eq!(non_ws[1].text, "456");
    }

    #[test]
    fn test_lex_operators() {
        let source = Arc::new(Source::from_raw("+ - * / == != < >"));
        let tokens = lex(source);
        let non_ws: Vec<_> = tokens
            .iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .collect();
        assert_eq!(non_ws[0].kind, TokenKind::Plus);
        assert_eq!(non_ws[1].kind, TokenKind::Minus);
        assert_eq!(non_ws[2].kind, TokenKind::Star);
        assert_eq!(non_ws[3].kind, TokenKind::Slash);
        assert_eq!(non_ws[4].kind, TokenKind::DoubleEquals);
        assert_eq!(non_ws[5].kind, TokenKind::NotEquals);
        assert_eq!(non_ws[6].kind, TokenKind::LessThan);
        assert_eq!(non_ws[7].kind, TokenKind::GreaterThan);
    }

    #[test]
    fn test_spans() {
        let source = Arc::new(Source::from_raw("fn add"));
        let tokens = lex(source.clone());
        assert_eq!(tokens[0].span.start_offset, 0);
        assert_eq!(tokens[0].span.length, 2); // "fn"
        assert_eq!(tokens[2].span.start_offset, 3);
        assert_eq!(tokens[2].span.length, 3); // "add"
    }
}
