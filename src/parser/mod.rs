use std::sync::Arc;

use crate::{
    Ast, Ident, Source, Span, Stmt,
    arena::Id,
    lexer::{Token, TokenKind, lex},
    parser::builder::AstBuilder,
};

mod builder;
mod decl;
mod expr;
mod grammar;
mod ty;

pub use grammar::{Grammar, OpInfo, Association};

/// Parse error with location information
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl ParseError {
    pub fn to_diagnostic(&self) -> crate::diagnostics::Diagnostic {
        use crate::diagnostics::{Diagnostic, Label, Severity};
        Diagnostic::new(Severity::Error, &self.message)
            .with_label(Label::primary(self.span.clone(), "here"))
    }
}

/// Recovery strategy levels
#[derive(Clone, Copy, Debug)]
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
pub struct Parser<'src> {
    tokens: Vec<Token<'src>>,
    pos: usize,
    pub builder: AstBuilder,
    errors: Vec<ParseError>,
    in_recovery: bool,
}

impl<'src> Parser<'src> {
    pub fn new(tokens: Vec<Token<'src>>) -> Self {
        Self {
            tokens,
            pos: 0,
            builder: AstBuilder::new(),
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

    pub fn peek_token(&self) -> &Token<'src> {
        &self.tokens[self.pos]
    }

    pub fn at(&self, kind: TokenKind) -> bool {
        self.peek() == kind
    }

    pub fn at_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }

    pub fn current_span(&self) -> Span {
        self.peek_token().span.clone()
    }

    pub fn previous_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span.clone()
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

    pub fn expect(&mut self, kind: TokenKind) -> Option<Span> {
        if self.at(kind) {
            let span = self.current_span();
            self.advance();
            self.in_recovery = false;
            Some(span)
        } else {
            self.error_expected(&[kind]);
            None
        }
    }

    // --- Error handling ---

    pub fn error(&mut self, message: String) {
        if !self.in_recovery {
            self.errors.push(ParseError {
                message,
                span: self.current_span(),
            });
        }
        self.in_recovery = true;
    }

    pub fn error_expected(&mut self, expected: &[TokenKind]) {
        let msg = if expected.len() == 1 {
            format!("expected {:?}, found {:?}", expected[0], self.peek())
        } else {
            format!("expected one of {:?}, found {:?}", expected, self.peek())
        };
        self.error(msg);
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
            RecoveryLevel::Declaration => &[
                TokenKind::Fn,
                TokenKind::Extern,
                TokenKind::CloseBrace,
            ],
        };

        while !self.at_eof() {
            if sync_tokens.contains(&self.peek()) {
                // For semicolons, consume them as part of recovery
                if self.at(TokenKind::Semicolon) {
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

    // --- Identifier parsing ---

    pub fn parse_ident(&mut self) -> Option<Ident> {
        if self.at(TokenKind::Ident) {
            let text = self.peek_token().text;
            self.advance();
            Some(self.builder.make_ident(text))
        } else {
            self.error_expected(&[TokenKind::Ident]);
            None
        }
    }

    // --- Finalization ---

    pub fn finish(self) -> (Ast, Vec<ParseError>) {
        (self.builder.into_ast(), self.errors)
    }
}

// --- Public API ---

/// Parse source code into an AST
pub fn parse(source: Arc<Source>) -> (Ast, Vec<ParseError>) {
    let tokens = lex(source);
    let mut parser = Parser::new(tokens);

    // Skip initial trivia
    parser.skip_trivia();

    // Parse the program
    parser.parse_program();

    parser.finish()
}
