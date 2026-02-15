use std::sync::Arc;

use crate::{
    Ast, Ident, Path, Source, Span, Stmt,
    arena::Id,
    lexer::{Token, TokenKind, lex},
};

mod builder;
mod decl;
mod error;
mod expr;
mod grammar;
mod stmt;
mod ty;

pub use builder::AstBuilder;
pub use error::ParseError;
pub use grammar::{Association, Grammar, OpInfo};

/// Recovery strategy levels
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryLevel {
    /// Sync at expression boundaries: ), ], }, ;, ,
    Expression,
    /// Sync at statement boundaries: ;, }, let, loop, while, if
    Statement,
    /// Sync at declaration boundaries: fn, struct, enum, extern
    Declaration,
}

/// Result of parsing something that could be a statement or trailing expression
pub enum StmtOrExpr {
    Stmt(Id<Stmt>),
    Expr(crate::arena::Id<crate::Expr>),
    Error,
}

/// Main parser struct
pub struct Parser<'ast> {
    tokens: Vec<Token>,
    pos: usize,
    pub builder: &'ast mut AstBuilder,
    errors: Vec<ParseError>,
    in_recovery: bool,
}

impl<'ast> Parser<'ast> {
    pub fn new(tokens: Vec<Token>, builder: &'ast mut AstBuilder) -> Self {
        Self {
            tokens,
            pos: 0,
            builder,
            errors: Vec::new(),
            in_recovery: false,
        }
    }

    // --- Token inspection ---

    pub fn peek(&self) -> TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::Eof)
    }

    pub fn peek_token(&self) -> &Token {
        &self.tokens[self.pos]
    }

    pub fn at(&self, kind: TokenKind) -> bool {
        self.peek() == kind
    }

    pub fn at_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }

    /// Peek at the next non-trivia token (one ahead of current)
    pub fn peek_next(&self) -> TokenKind {
        self.peek_nth(1)
    }

    /// Peek at the nth non-trivia token ahead (0 = current, 1 = next, etc.)
    pub fn peek_nth(&self, n: usize) -> TokenKind {
        let mut pos = self.pos;
        let mut count = 0;
        while pos < self.tokens.len() {
            let kind = self.tokens[pos].kind;
            if !matches!(kind, TokenKind::Whitespace | TokenKind::Comment) {
                if count == n {
                    return kind;
                }
                count += 1;
            }
            pos += 1;
        }
        TokenKind::Eof
    }

    pub fn current_span(&self) -> Span {
        self.peek_token().span.clone()
    }

    pub fn previous_span(&self) -> Span {
        let mut pos = self.pos.saturating_sub(1);
        while pos > 0
            && matches!(
                self.tokens[pos].kind,
                TokenKind::Whitespace | TokenKind::Comment
            )
        {
            pos -= 1;
        }
        self.tokens[pos].span.clone()
    }

    // --- Token consumption ---

    pub fn advance(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
        self.skip_trivia();
    }

    fn skip_trivia(&mut self) {
        while matches!(self.peek(), TokenKind::Whitespace | TokenKind::Comment) {
            self.pos += 1;
        }
    }

    pub fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            self.in_recovery = false;
            true
        } else {
            false
        }
    }

    pub fn expect_closing(&mut self, kind: TokenKind, expected: impl Into<String>) -> Option<Span> {
        if self.at(kind) {
            let span = self.current_span();
            self.advance();
            self.in_recovery = false;
            Some(span)
        } else {
            if !self.in_recovery {
                self.errors.push(ParseError {
                    message: format!("missing {kind}"),
                    hint: format!("expected {}", expected.into()),
                    span: self.previous_span().after(),
                });
            }
            self.in_recovery = true;
            None
        }
    }

    // --- Error handling ---

    pub fn error_missing(&mut self, message: impl Into<String>, expected: impl Into<String>) {
        if !self.in_recovery {
            self.errors.push(ParseError {
                message: message.into(),
                hint: format!("expected {}", expected.into()),
                span: self.previous_span().after(),
            });
        }
        self.in_recovery = true;
    }

    pub fn error_expected(&mut self, expected: &[TokenKind], expected_desc: impl Into<String>) {
        let expected_desc = expected_desc.into();
        let msg = if expected.len() == 1 {
            format!("expected {}, found {}", expected[0], self.peek())
        } else {
            format!(
                "expected one of {}, found {}",
                expected
                    .iter()
                    .map(|k| k.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                self.peek()
            )
        };
        self.error_missing(msg, expected_desc);
    }

    // --- Error recovery ---

    pub fn recover(&mut self, level: RecoveryLevel) {
        let sync_tokens: &[TokenKind] = match level {
            RecoveryLevel::Expression => &[
                TokenKind::Semicolon,
                TokenKind::Comma,
                TokenKind::CloseParen,
                TokenKind::CloseBrace,
            ],
            RecoveryLevel::Statement => &[
                TokenKind::Semicolon,
                TokenKind::CloseBrace,
                TokenKind::Let,
                TokenKind::Loop,
                TokenKind::While,
                TokenKind::If,
                TokenKind::Fn,
            ],
            RecoveryLevel::Declaration => &[TokenKind::Fn, TokenKind::Extern, TokenKind::Struct],
        };

        while !self.at_eof() {
            if sync_tokens.contains(&self.peek()) {
                // For semicolons, consume them as part of recovery
                if self.at(TokenKind::Semicolon) {
                    self.advance();
                }
                // For close braces at declaration level, consume to avoid infinite loop
                // (parse_program doesn't handle stray close braces)
                if level == RecoveryLevel::Declaration && self.at(TokenKind::CloseBrace) {
                    self.advance();
                }
                self.in_recovery = false;
                return;
            }

            // Skip balanced delimiters
            match self.peek() {
                TokenKind::OpenBrace => {
                    self.skip_balanced(TokenKind::OpenBrace, TokenKind::CloseBrace);
                }
                TokenKind::OpenParen => {
                    self.skip_balanced(TokenKind::OpenParen, TokenKind::CloseParen);
                }
                _ => self.advance(),
            }
        }
    }

    fn skip_balanced(&mut self, open: TokenKind, close: TokenKind) {
        assert!(self.at(open));
        let mut depth = 0;

        loop {
            if self.at(open) {
                depth += 1;
            } else if self.at(close) {
                depth -= 1;
                if depth == 0 {
                    self.advance();
                    return;
                }
            } else if self.at_eof() {
                return;
            }
            self.advance();
        }
    }

    pub fn parse_ident(&mut self, expected: impl Into<String>) -> Option<Ident> {
        if self.at(TokenKind::Ident) {
            let text = self.peek_token().text.clone();
            self.advance();
            Some(self.builder.make_ident(&text))
        } else {
            self.error_expected(&[TokenKind::Ident], expected);
            None
        }
    }

    pub fn parse_path(&mut self, expected: impl Into<String>) -> Option<Path> {
        let expected = expected.into();
        let path = self
            .parse_separated_while(TokenKind::DoubleColon, |p| p.parse_ident(expected.clone()))?;
        Some(Path(path))
    }

    pub fn parse_separated<T>(
        &mut self,
        separator: TokenKind,
        terminator: TokenKind, // e.g., CloseParen
        mut parse_item: impl FnMut(&mut Self) -> Option<T>,
        expected: impl Into<String>,
    ) -> Option<Vec<T>> {
        let expected = expected.into();
        let mut items = vec![];

        while !self.at(terminator) {
            if items.len() > 0 {
                self.expect_closing(separator, expected.clone())?;
                if self.at(terminator) {
                    break; // trailing separator                           
                }
            }

            items.push(parse_item(self)?);
        }

        Some(items)
    }

    pub fn parse_separated_while<T>(
        &mut self,
        separator: TokenKind,
        mut parse_item: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<Vec<T>> {
        let mut items = vec![];

        loop {
            items.push(parse_item(self)?);

            if !self.eat(separator) {
                break;
            }
        }

        Some(items)
    }

    pub fn finish(self) -> Vec<ParseError> {
        self.errors
    }
}

pub fn parse(source: Arc<Source>) -> (Ast, Vec<ParseError>) {
    let tokens = lex(source);
    let mut builder = AstBuilder::new();

    // Create a default module for standalone parsing (empty name = no prefix)
    builder.start_module("", std::path::PathBuf::new());

    let mut parser = Parser::new(tokens, &mut builder);

    parser.skip_trivia();
    parser.parse_program();

    let errors = parser.finish();
    let ast = builder.finish();

    (ast, errors)
}

pub fn parse_module(
    source: Arc<Source>,
    builder: &mut AstBuilder,
    module_name: &str,
    module_path: std::path::PathBuf,
) -> Vec<ParseError> {
    builder.start_module(module_name, module_path);

    let tokens = lex(source);
    let mut parser = Parser::new(tokens, builder);

    parser.skip_trivia();
    parser.parse_program();

    parser.finish()
}
