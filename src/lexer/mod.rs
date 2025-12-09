use logos::Logos;
use crate::span::Span;

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // Keywords
    #[token("fn")]
    FnKw,
    #[token("let")]
    LetKw,
    #[token("if")]
    IfKw,
    #[token("else")]
    ElseKw,

    // Literals and identifiers
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident,
    #[regex(r"[0-9]+")]
    Int,

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
    Eq,
    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,

    // Delimiters
    #[token("(")]
    LeftParen,
    #[token(")")]
    RightParen,
    #[token("{")]
    LeftBrace,
    #[token("}")]
    RightBrace,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token(":")]
    Colon,
    #[token("->")]
    Arrow,

    // Whitespace and comments (skipped during parsing)
    #[regex(r"[ \t\r\n]+")]
    Whitespace,
    #[regex(r"//[^\n]*")]
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

pub fn lex(input: &str) -> Vec<Token<'_>> {
    let mut lexer = TokenKind::lexer(input);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        let kind = result.unwrap_or(TokenKind::Error);
        let span = lexer.span();
        let text = lexer.slice();
        tokens.push(Token {
            kind,
            text,
            span: Span::from_range(span),
        });
    }

    // Add EOF token
    let eof_pos = input.len() as u32;
    tokens.push(Token {
        kind: TokenKind::Eof,
        text: "",
        span: Span::new(eof_pos, eof_pos),
    });

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_keywords() {
        let tokens = lex("fn let if else");
        let non_ws: Vec<_> = tokens.iter().filter(|t| t.kind != TokenKind::Whitespace).collect();
        assert_eq!(non_ws[0].kind, TokenKind::FnKw);
        assert_eq!(non_ws[1].kind, TokenKind::LetKw);
        assert_eq!(non_ws[2].kind, TokenKind::IfKw);
        assert_eq!(non_ws[3].kind, TokenKind::ElseKw);
    }

    #[test]
    fn test_lex_identifiers() {
        let tokens = lex("foo bar_baz x123");
        let non_ws: Vec<_> = tokens.iter().filter(|t| t.kind != TokenKind::Whitespace).collect();
        assert_eq!(non_ws[0].kind, TokenKind::Ident);
        assert_eq!(non_ws[0].text, "foo");
        assert_eq!(non_ws[1].kind, TokenKind::Ident);
        assert_eq!(non_ws[1].text, "bar_baz");
    }

    #[test]
    fn test_lex_integers() {
        let tokens = lex("123 456");
        let non_ws: Vec<_> = tokens.iter().filter(|t| t.kind != TokenKind::Whitespace).collect();
        assert_eq!(non_ws[0].kind, TokenKind::Int);
        assert_eq!(non_ws[0].text, "123");
        assert_eq!(non_ws[1].kind, TokenKind::Int);
        assert_eq!(non_ws[1].text, "456");
    }

    #[test]
    fn test_lex_operators() {
        let tokens = lex("+ - * / == != < >");
        let non_ws: Vec<_> = tokens.iter().filter(|t| t.kind != TokenKind::Whitespace).collect();
        assert_eq!(non_ws[0].kind, TokenKind::Plus);
        assert_eq!(non_ws[1].kind, TokenKind::Minus);
        assert_eq!(non_ws[2].kind, TokenKind::Star);
        assert_eq!(non_ws[3].kind, TokenKind::Slash);
        assert_eq!(non_ws[4].kind, TokenKind::EqEq);
        assert_eq!(non_ws[5].kind, TokenKind::NotEq);
        assert_eq!(non_ws[6].kind, TokenKind::Lt);
        assert_eq!(non_ws[7].kind, TokenKind::Gt);
    }

    #[test]
    fn test_spans() {
        let tokens = lex("fn add");
        assert_eq!(tokens[0].span, Span::new(0, 2)); // "fn"
        assert_eq!(tokens[2].span, Span::new(3, 6)); // "add"
    }
}
