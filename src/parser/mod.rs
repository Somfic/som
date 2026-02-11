use crate::ast::{Ast, Ident};
use crate::lexer::{Token, TokenKind, lex};
use crate::span::{Source, Span};
use std::sync::Arc;

mod error;
pub use error::*;

mod decl;
mod expr;
mod stmt;
mod ty;

pub struct Parser<'src> {
    tokens: Vec<Token<'src>>,
    pos: usize,
    ast: Ast,
    errors: Vec<ParseError>,
    next_ident_id: u32,
}

impl<'src> Parser<'src> {
    pub fn new(tokens: Vec<Token<'src>>) -> Self {
        Self {
            tokens,
            pos: 0,
            ast: Ast::new(),
            errors: Vec::new(),
            next_ident_id: 0,
        }
    }

    pub fn parse(mut self) -> (Ast, Vec<ParseError>) {
        self.parse_program();
        (self.ast, self.errors)
    }

    // Token manipulation

    fn peek(&self) -> TokenKind {
        self.peek_token().kind
    }

    fn peek_token(&self) -> &Token<'src> {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().unwrap())
    }

    fn peek_span(&self) -> Span {
        self.peek_token().span.clone()
    }

    fn previous_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span.clone()
        } else {
            // Return an empty span at the start of the source
            Span::empty(self.tokens[0].span.source.clone())
        }
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek() == kind
    }

    fn at_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }

    fn advance(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
        // Skip whitespace and comments
        self.skip_trivia();
    }

    fn skip_trivia(&mut self) {
        while matches!(self.peek(), TokenKind::Whitespace | TokenKind::Comment) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Span> {
        if self.at(kind) {
            let span = self.peek_span();
            self.advance();
            Some(span)
        } else {
            self.error(vec![kind]);
            None
        }
    }

    fn error(&mut self, expected: Vec<TokenKind>) {
        let span = self.peek_span();
        let found = self.peek();
        self.errors.push(ParseError::new(expected, found, span));
        self.synchronize();
    }

    // Skip tokens until we reach a synchronization point
    fn synchronize(&mut self) {
        while !self.at_eof() {
            // Stop at statement/declaration boundaries
            match self.peek() {
                TokenKind::Fn | TokenKind::Let | TokenKind::CloseBrace => return,
                _ => self.advance(),
            }
        }
    }

    // Helpers

    fn make_ident(&mut self, text: &str) -> Ident {
        let id = self.next_ident_id;
        self.next_ident_id += 1;
        Ident {
            id,
            value: text.into(),
        }
    }

    fn parse_ident(&mut self) -> Option<(Ident, Span)> {
        if self.at(TokenKind::Ident) {
            let token = self.peek_token();
            let span = token.span.clone();
            let text = token.text.to_owned();
            self.advance();
            Some((self.make_ident(&text), span))
        } else {
            self.error(vec![TokenKind::Ident]);
            None
        }
    }
}

/// Parse source code into an AST
pub fn parse(source: Arc<Source>) -> (Ast, Vec<ParseError>) {
    let tokens = lex(source);
    let mut parser = Parser::new(tokens);
    parser.skip_trivia(); // Skip leading whitespace
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let source = Arc::new(Source::from_raw("fn add(x: i32, y: i32) -> i32 { x + y }"));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
        assert_eq!(ast.funcs.len(), 1);
    }

    #[test]
    fn test_parse_function_with_let() {
        let source = Arc::new(Source::from_raw("fn test() { let x: i32 = 5; x }"));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_binary_expr() {
        let source = Arc::new(Source::from_raw("fn test() { 1 + 2 * 3 }"));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_function_call() {
        // Define add before test, so it's available for name resolution
        let source = Arc::new(Source::from_raw(
            "fn add(a: i32, b: i32) -> i32 { a + b } fn test() { add(1, 2) }",
        ));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
        assert_eq!(ast.funcs.len(), 2);
    }

    #[test]
    fn test_parse_conditional_basic() {
        let source = Arc::new(Source::from_raw("fn test() -> i32 { 1 if true else 2 }"));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_conditional_with_expressions() {
        let source = Arc::new(Source::from_raw("fn test() -> i32 { 1 + 2 if true else 3 + 4 }"));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_conditional_nested() {
        let source = Arc::new(Source::from_raw(
            "fn test() -> i32 { 1 if true else (2 if false else 3) }",
        ));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_conditional_in_let() {
        let source = Arc::new(Source::from_raw("fn test() { let x = 1 if true else 2; }"));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_conditional_with_comparison() {
        let source = Arc::new(Source::from_raw("fn test(x: i32) -> i32 { 1 if x > 0 else 2 }"));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_conditional_precedence_addition() {
        // Should parse as (1 + 2) if true else (3 + 4), not 1 + (2 if true else 3) + 4
        let source = Arc::new(Source::from_raw("fn test() -> i32 { 1 + 2 if true else 3 + 4 }"));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_conditional_precedence_multiply() {
        // Should parse as (2 * 3) if true else (4 * 5)
        let source = Arc::new(Source::from_raw("fn test() -> i32 { 2 * 3 if true else 4 * 5 }"));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_conditional_precedence_mixed() {
        // Should parse as (1 + 2 * 3) if (x > 0) else (4 - 5)
        let source = Arc::new(Source::from_raw(
            "fn test(x: i32) -> i32 { 1 + 2 * 3 if x > 0 else 4 - 5 }",
        ));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }

    #[test]
    fn test_parse_conditional_chained() {
        // a if b else c if d else e should parse as a if b else (c if d else e)
        let source = Arc::new(Source::from_raw(
            "fn test(x: bool, y: bool) -> i32 { 1 if x else 2 if y else 3 }",
        ));
        let (ast, errors) = parse(source);
        assert!(errors.is_empty(), "Errors: {:?}", errors);
    }
}
