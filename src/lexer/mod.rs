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
    #[token("struct")]
    Struct,
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
    #[token("!")]
    Bang,
    #[token("<")]
    LessThan,
    #[token(">")]
    GreaterThan,
    #[token("<=")]
    LessThanOrEqual,
    #[token(">=")]
    GreaterThanOrEqual,

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

    // Loops
    #[token("loop")]
    Loop,
    #[token("while")]
    While,
    #[token("for")]
    For,

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
