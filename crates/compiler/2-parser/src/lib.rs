//! Tokens to syntax tree: a recursive-descent parser with a Pratt parser for
//! expressions, and error recovery at expression, statement and declaration level.

mod error;
mod expr;
mod grammar;
mod item;
mod stmt;
mod ty;

pub use error::ParseError;

use som_ast::{Ast, Ident, Item, Path, Span};
use som_base::{Message, Source, msg};
use som_lexer::{Token, TokenKind, lex};
use std::path::PathBuf;
use std::sync::Arc;

/// Parses a single source as the root module of a new program.
pub fn parse(source: Arc<Source>) -> (Ast, Vec<ParseError>) {
    let mut ast = Ast::default();
    let root = ast.add_module("", PathBuf::new());
    let errors = parse_file(source, &mut ast, root);
    (ast, errors)
}

/// Parses one source file, adding its items to an existing module of `ast`.
pub fn parse_file(source: Arc<Source>, ast: &mut Ast, module: usize) -> Vec<ParseError> {
    ast.modules[module].files.push(source.clone());
    let mut parser = Parser {
        tokens: lex(source),
        pos: 0,
        ast,
        module,
        errors: Vec::new(),
        in_recovery: false,
        in_condition: false,
    };
    parser.skip_trivia();
    parser.parse_program();
    parser.errors
}

/// How far to skip ahead when recovering from a parse error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoveryLevel {
    /// Sync at `;`, `}`, `let`, `loop`, `while`, `if`, `fn`
    Statement,
    /// Sync at `fn`, `extern`, `struct`
    Declaration,
    /// Inside an `impl` or `trait` block: sync at `fn`, `type`, or the block's `}`, which is
    /// left for the block.
    Member,
}

/// Something that could be either a statement or a trailing (value) expression.
enum StmtOrExpr {
    Stmt(som_ast::Id<som_ast::Stmt>),
    Expr(som_ast::Id<som_ast::Expr>),
    Error,
}

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    ast: &'a mut Ast,
    module: usize,
    errors: Vec<ParseError>,
    in_recovery: bool,
    /// Parsing the condition of an `if` or `while`, where `name {` starts the body, not a
    /// constructor. Cleared again inside parentheses and blocks.
    in_condition: bool,
}

impl Parser<'_> {
    fn add_item(&mut self, item: Item) {
        self.ast.modules[self.module].items.push(item);
    }

    // --- Token inspection ---

    fn peek(&self) -> TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn peek_token(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek() == kind
    }

    fn at_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }

    /// The kind of the nth non-trivia token ahead (0 = current).
    fn peek_nth(&self, n: usize) -> TokenKind {
        self.tokens[self.pos..]
            .iter()
            .filter(|t| !is_trivia(t.kind))
            .nth(n)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::Eof)
    }

    /// The `///` comment right above the current token: consecutive `///` lines, joined.
    /// A blank line or an ordinary comment in between ends it.
    fn doc_comment(&self) -> Option<String> {
        let mut lines = Vec::new();
        for token in self.tokens[..self.pos].iter().rev() {
            match token.kind {
                TokenKind::Whitespace if token.text.matches('\n').count() <= 1 => continue,
                TokenKind::Comment
                    if token.text.starts_with("///") && !token.text.starts_with("////") =>
                {
                    let text = &token.text[3..];
                    lines.push(
                        text.strip_prefix(' ')
                            .unwrap_or(text)
                            .trim_end()
                            .to_string(),
                    );
                }
                _ => break,
            }
        }
        lines.reverse();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    fn current_span(&self) -> Span {
        self.peek_token().span.clone()
    }

    fn previous_span(&self) -> Span {
        let mut pos = self.pos.saturating_sub(1);
        while pos > 0 && is_trivia(self.tokens[pos].kind) {
            pos -= 1;
        }
        self.tokens[pos].span.clone()
    }

    /// The span from `start` up to the end of the last consumed token.
    fn span_from(&self, start: &Span) -> Span {
        start.merge(&self.previous_span())
    }

    // --- Token consumption ---

    fn advance(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
        self.skip_trivia();
    }

    fn skip_trivia(&mut self) {
        while is_trivia(self.peek()) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            self.in_recovery = false;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind, expected: impl Into<String>) -> Option<Span> {
        if self.at(kind) {
            let span = self.current_span();
            self.advance();
            self.in_recovery = false;
            Some(span)
        } else {
            if !self.in_recovery {
                self.errors.push(ParseError {
                    message: msg!["missing ", kind.part()],
                    hint: msg!["expected ", expected.into()],
                    span: self.previous_span().after(),
                });
            }
            self.in_recovery = true;
            None
        }
    }

    // --- Errors ---

    fn error_missing(&mut self, message: impl Into<Message>, expected: impl Into<String>) {
        if !self.in_recovery {
            self.errors.push(ParseError {
                message: message.into(),
                hint: msg!["expected ", expected.into()],
                span: self.previous_span().after(),
            });
        }
        self.in_recovery = true;
    }

    fn error_expected(&mut self, expected: &[TokenKind], expected_desc: impl Into<String>) {
        let found = self.peek();
        let mut message = match expected {
            [one] => msg!["expected ", one.part()],
            many => {
                let mut message = msg!["expected one of "];
                for (i, kind) in many.iter().enumerate() {
                    if i > 0 {
                        message.0.push(", ".into());
                    }
                    message.0.push(kind.part());
                }
                message
            }
        };
        message.0.push(", found ".into());
        message.0.push(found.part());
        self.error_missing(message, expected_desc);
    }

    fn error_at(&mut self, span: Span, message: impl Into<Message>, hint: impl Into<Message>) {
        self.errors.push(ParseError {
            message: message.into(),
            hint: hint.into(),
            span,
        });
    }

    // --- Recovery ---

    fn recover(&mut self, level: RecoveryLevel) {
        let sync_tokens: &[TokenKind] = match level {
            RecoveryLevel::Statement => &[
                TokenKind::Semicolon,
                TokenKind::CloseBrace,
                TokenKind::Let,
                TokenKind::Loop,
                TokenKind::While,
                TokenKind::Break,
                TokenKind::If,
                TokenKind::Fn,
            ],
            RecoveryLevel::Member => &[TokenKind::Fn, TokenKind::Type, TokenKind::CloseBrace],
            RecoveryLevel::Declaration => &[
                TokenKind::Fn,
                TokenKind::Extern,
                TokenKind::Struct,
                TokenKind::Enum,
                TokenKind::Trait,
                TokenKind::Impl,
            ],
        };

        while !self.at_eof() {
            if sync_tokens.contains(&self.peek()) {
                if self.at(TokenKind::Semicolon) {
                    self.advance();
                }
                // The declaration loop doesn't handle stray `}`; consume it to avoid looping forever.
                if level == RecoveryLevel::Declaration && self.at(TokenKind::CloseBrace) {
                    self.advance();
                }
                self.in_recovery = false;
                return;
            }

            match self.peek() {
                TokenKind::OpenBrace => {
                    self.skip_balanced(TokenKind::OpenBrace, TokenKind::CloseBrace)
                }
                TokenKind::OpenParen => {
                    self.skip_balanced(TokenKind::OpenParen, TokenKind::CloseParen)
                }
                _ => self.advance(),
            }
        }
    }

    fn skip_balanced(&mut self, open: TokenKind, close: TokenKind) {
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

    // --- Shared pieces ---

    fn parse_ident(&mut self, expected: impl Into<String>) -> Option<Ident> {
        if self.at(TokenKind::Ident) {
            let token = self.peek_token();
            let ident = Ident {
                name: token.text.clone(),
                span: token.span.clone(),
            };
            self.advance();
            Some(ident)
        } else {
            self.error_expected(&[TokenKind::Ident], expected);
            None
        }
    }

    fn parse_path(&mut self, expected: impl Into<String>) -> Option<Path> {
        let expected = expected.into();
        let mut segments = vec![self.parse_ident(expected.clone())?];
        while self.eat(TokenKind::DoubleColon) {
            segments.push(self.parse_ident(expected.clone())?);
        }
        Some(Path { segments })
    }

    /// Parses `item, item, item` up to (not including) `close`, allowing a trailing comma.
    fn parse_comma_list<T>(
        &mut self,
        close: TokenKind,
        mut parse_item: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<Vec<T>> {
        let mut items = Vec::new();
        while !self.at(close) && !self.at_eof() {
            items.push(parse_item(self)?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        Some(items)
    }
}

fn is_trivia(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Whitespace | TokenKind::Comment)
}
