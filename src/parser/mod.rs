use crate::ast::{Ast, Ident};
use crate::lexer::{Token, TokenKind, lex};
use crate::span::{Source, Span};
use std::path::Path;
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

    /// Resolve a library path relative to the source file's directory.
    /// Returns an absolute path if possible for proper runtime library loading.
    fn resolve_library_path(&self, lib_path: &str, span: &Span) -> String {
        let path = Path::new(lib_path);

        // If the path is already absolute, return as-is
        if path.is_absolute() {
            return lib_path.to_string();
        }

        // Try to get the source file's directory
        if let Source::File(source_path, _) = span.source.as_ref() {
            // First, try to get an absolute path for the source file
            let absolute_source = source_path
                .canonicalize()
                .unwrap_or_else(|_| source_path.clone());

            if let Some(source_dir) = absolute_source.parent() {
                let resolved = source_dir.join(path);
                // Try to canonicalize the resolved path (requires file to exist)
                if let Ok(canonical) = resolved.canonicalize() {
                    return canonical.to_string_lossy().to_string();
                }
                return resolved.to_string_lossy().to_string();
            }
        }

        // Fallback: return the original path
        lib_path.to_string()
    }
}

/// Parse source code into an AST
pub fn parse(source: Arc<Source>) -> (Ast, Vec<ParseError>) {
    let tokens = lex(source);
    let mut parser = Parser::new(tokens);
    parser.skip_trivia(); // Skip leading whitespace
    parser.parse()
}
