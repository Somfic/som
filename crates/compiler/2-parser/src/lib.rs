//! Tokens to syntax tree: a recursive-descent parser with a Pratt parser for
//! expressions, and error recovery at expression, statement and declaration level.

mod error;
mod expand;
mod expr;
mod foreign;
mod grammar;
mod item;
mod stmt;
mod ty;

pub use error::ParseError;
pub use expand::expand_reflected;

use som_ast::{Ast, Ident, Item, Path, Span};
use som_base::message::code;
use som_base::{Fix, Message, Source, msg};
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
    let mut parser = Parser::new(lex(source), ast, module);
    parser.report_foreign_comments();
    parser.skip_trivia();
    parser.parse_program();
    parser.errors
}

impl<'a> Parser<'a> {
    /// A parser of `tokens`, which end in an `Eof`, adding what it reads to `module`.
    fn new(tokens: Vec<Token>, ast: &'a mut Ast, module: usize) -> Self {
        Parser {
            tokens,
            pos: 0,
            ast,
            module,
            errors: Vec::new(),
            in_recovery: false,
            in_condition: false,
            in_settled: false,
            in_impl: false,
            in_params: false,
            foreign_depth: 0,
            foreign_semicolons: Vec::new(),
            visibility: som_ast::Visibility::Private,
            in_impl_target: false,
            wheres: Vec::new(),
            listens: Vec::new(),
            when: Vec::new(),
            when_frames: Vec::new(),
            where_doc: None,
            where_start: None,
            trait_supers: Vec::new(),
        }
    }
}

/// The value an integer literal is written as: decimal, `0x` hexadecimal or `0b` binary, with
/// `_` between digits ignored. `None` if it doesn't fit in 128 bits.
pub(crate) fn int_value(text: &str) -> Option<u128> {
    let (digits, radix) = match text.get(..2) {
        Some("0x") => (&text[2..], 16),
        Some("0b") => (&text[2..], 2),
        _ => (text, 10),
    };
    let digits: String = digits.chars().filter(|&c| c != '_').collect();
    u128::from_str_radix(&digits, radix).ok()
}

/// A fix putting the missing `kind` in at `at`, for punctuation, which is always written the
/// same way: a forgotten `;`, `,` or `)`.
fn insertion(kind: TokenKind, at: &Span) -> Option<som_base::Fix> {
    let text = match kind {
        TokenKind::Semicolon => ";",
        TokenKind::Comma => ",",
        TokenKind::Colon => ":",
        TokenKind::CloseParen => ")",
        TokenKind::CloseBracket => "]",
        TokenKind::CloseBrace => "}",
        TokenKind::GreaterThan => ">",
        TokenKind::FatArrow => " =>",
        TokenKind::Equals => " =",
        _ => return None,
    };
    Some(som_base::Fix {
        title: msg!["add the missing ", som_base::message::code(text.trim())],
        edits: vec![(at.before(), text.to_string())],
    })
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
    /// In an `@if`'s condition, where a type can be compared with another: `T == f32`.
    in_settled: bool,
    /// In an `impl` or `trait` block, where methods can take `self`.
    in_impl: bool,
    /// In a function's parameter list, where `->` after a parameter means its `)` is missing.
    in_params: bool,
    /// How many bodies written without braces (`then … end`, Python's `:`) this is in, where
    /// a line's missing `;` is noted, in `foreign_semicolons`, for the body's fix.
    foreign_depth: usize,
    foreign_semicolons: Vec<Span>,
    /// The `pub` or `mod` just read, for the item, method or field after it to take.
    visibility: som_ast::Visibility,
    /// Parsing what an `impl` is for, where a slice's elements can be named bare, `[T]`:
    /// `impl Index<usize> for [T]`, whose `&self` is the slice.
    in_impl_target: bool,
    /// `@where` conditions read, for the function after them, with the comment above them
    /// and where they start.
    wheres: Vec<som_ast::Where>,
    /// `@on` events read, for the function after them.
    listens: Vec<som_ast::Ident>,
    /// The `@if`s among an impl block's members the parser is inside: what the members after
    /// them need to hold.
    when: Vec<som_ast::Id<som_ast::Expr>>,
    /// For each `@if` among the members the parser is inside: how many of `when` it added
    /// (`else @if` adds the earlier conditions, not holding), and whether the last of them is
    /// its own condition, which an `else` after it turns around.
    when_frames: Vec<(usize, bool)>,
    where_doc: Option<String>,
    where_start: Option<Span>,
    /// The traits the trait being read builds on, which its methods' `Self` implements.
    trait_supers: Vec<som_ast::Path>,
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

    /// The name the nth non-trivia token ahead (0 = current) is, if it's one: `Self` too.
    fn ident_ahead(&self, n: usize) -> Option<&str> {
        (self.tokens[self.pos..].iter())
            .filter(|t| !is_trivia(t.kind))
            .nth(n)
            .filter(|t| matches!(t.kind, TokenKind::Ident | TokenKind::SelfType))
            .map(|t| &*t.text)
    }

    /// The span of the nth non-trivia token ahead (0 = current).
    fn span_ahead(&self, n: usize) -> Span {
        (self.tokens[self.pos..].iter())
            .filter(|t| !is_trivia(t.kind))
            .nth(n)
            .map_or_else(|| self.current_span(), |t| t.span.clone())
    }

    /// Whether the nth non-trivia token ahead (0 = current) is `self`.
    fn self_ahead(&self, n: usize) -> bool {
        self.peek_nth(n) == TokenKind::SelfValue
    }

    /// `self` as a name: a method's first parameter, or the value it's called on.
    fn parse_self(&mut self) -> Ident {
        let span = self.current_span();
        self.advance();
        Ident {
            name: som_lang::SELF_VALUE.into(),
            span,
        }
    }

    /// The `///` comment right above the current token: consecutive `///` lines, joined.
    /// A blank line or an ordinary comment in between ends it.
    fn doc_comment(&self) -> Option<String> {
        let mut lines = Vec::new();
        for token in self.tokens[..self.pos].iter().rev() {
            match token.kind {
                TokenKind::Whitespace if token.text.matches('\n').count() <= 1 => continue,
                // `/// …` above `pub fn`: the comment is the item's.
                TokenKind::Pub | TokenKind::Mod => continue,
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

    /// `pub` or `mod` before an item, method or field, if it's there: who can see it.
    fn parse_visibility(&mut self) -> som_ast::Visibility {
        // `public static void main`: other languages' modifiers, reported, and `public` read
        // as the `pub` it means.
        let public = self.eat_foreign_modifiers();
        if public {
            self.eat(TokenKind::Pub);
            som_ast::Visibility::Public
        } else if self.eat(TokenKind::Pub) {
            som_ast::Visibility::Public
        } else if self.eat(TokenKind::Mod) {
            som_ast::Visibility::Module
        } else {
            som_ast::Visibility::Private
        }
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
            self.error_missing_token(msg!["missing ", kind.part()], expected, Some(kind));
            None
        }
    }

    // --- Errors ---

    fn error_missing(&mut self, message: impl Into<Message>, expected: impl Into<String>) {
        self.error_missing_token(message, expected, None);
    }

    /// Like `error_missing`, for a missing token `missing`: the fix puts it in, if it's one
    /// that's always written the same way.
    fn error_missing_token(
        &mut self,
        message: impl Into<Message>,
        expected: impl Into<String>,
        missing: Option<TokenKind>,
    ) {
        // In a body written without braces, a line's `;` is left out: noted for its fix.
        if missing == Some(TokenKind::Semicolon) && self.foreign_semicolon() {
            return;
        }
        if !self.in_recovery {
            // What's missing goes right after the previous token, but point at what's there
            // instead when it's on the same line: `const name` is wrong at `name`, not at the
            // space before it. On a later line, the gap is where the line ended.
            let after = self.previous_span().after();
            let found = self.current_span();
            // A token written in by an `@for` from its list, `12` in `@for T in [u8, 12]`, is
            // where it's written: it's what's wrong, not a gap after what's before it.
            let elsewhere = found.source.identifier() != after.source.identifier()
                || found.start_offset < after.start_offset;
            let span = if !self.at_eof() && (found.start.line == after.end.line || elsewhere) {
                found.clone()
            } else {
                after.clone()
            };
            let fix = match missing {
                _ if elsewhere => None,
                // `Circle(r) -> r`: the look-alike written in its place, changed.
                Some(kind) if let Some(text) = self.look_alike(kind) => Some(Fix::replace(
                    msg!["change ", code(self.peek().to_string()), " to ", code(text)],
                    found.clone(),
                    text,
                )),
                Some(kind) if self.insertion_fits(kind) => insertion(kind, &after),
                _ => None,
            };
            self.errors.push(ParseError {
                message: message.into(),
                hint: msg!["expected ", expected.into()],
                fix,
                span,
            });
        }
        self.in_recovery = true;
    }

    /// Whether putting a missing `kind` in before the current token makes sense of what
    /// follows. Not when what follows shows something else is wrong: `use std.io` isn't
    /// missing a `;` before `.`, nor `(a, b) => …` a `)` before `,`.
    /// The text of `kind`, missing, if what's there instead is a look-alike of it: `->` for
    /// `=>`, `=>` for `->`.
    fn look_alike(&self, kind: TokenKind) -> Option<&'static str> {
        match (kind, self.peek()) {
            (TokenKind::FatArrow, TokenKind::Arrow) => Some("=>"),
            (TokenKind::Arrow, TokenKind::FatArrow) => Some("->"),
            _ => None,
        }
    }

    fn insertion_fits(&self, kind: TokenKind) -> bool {
        let found = self.peek();
        // At the end of a line, or of the file, the token's just missing.
        if found == TokenKind::Eof || self.current_span().start.line > self.previous_span().end.line
        {
            return true;
        }
        match kind {
            TokenKind::Semicolon => {
                self.at_item()
                    || matches!(
                        found,
                        TokenKind::Let
                            | TokenKind::Return
                            | TokenKind::If
                            | TokenKind::While
                            | TokenKind::For
                            | TokenKind::Loop
                            | TokenKind::Match
                            | TokenKind::Break
                            | TokenKind::Continue
                            | TokenKind::CloseBrace
                    )
            }
            TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace => {
                matches!(
                    found,
                    TokenKind::Semicolon
                        | TokenKind::OpenBrace
                        | TokenKind::CloseParen
                        | TokenKind::CloseBracket
                        | TokenKind::CloseBrace
                ) || (kind == TokenKind::CloseParen
                    // `fn f(a: i32 -> i32`, but not `map(fun x -> x)`.
                    && self.in_params
                    && found == TokenKind::Arrow)
            }
            // `x -> …` and `x => …` start a lambda, not the next item: `map(fun x -> x)`.
            TokenKind::Comma
                if found == TokenKind::Ident
                    && matches!(self.peek_nth(1), TokenKind::Arrow | TokenKind::FatArrow) =>
            {
                false
            }
            // `let mutable total = 0`, `let add a b = …`: the `=` is there, later; what's
            // before it is what's wrong.
            TokenKind::Equals => !self.equals_ahead(),
            // `f(x!)`: an operator with nothing after it isn't the start of the next item.
            TokenKind::Comma => {
                !matches!(
                    self.peek_nth(1),
                    TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace
                ) || !matches!(
                    found,
                    TokenKind::Bang
                        | TokenKind::Minus
                        | TokenKind::Star
                        | TokenKind::Ampersand
                        | TokenKind::Tilde
                )
            }
            _ => true,
        }
    }

    /// Whether a `=` comes later in this statement, on this line.
    fn equals_ahead(&self) -> bool {
        let line = self.current_span().start.line;
        (self.tokens[self.pos..].iter())
            .filter(|t| !is_trivia(t.kind))
            .take_while(|t| t.span.start.line == line && t.kind != TokenKind::Semicolon)
            .any(|t| t.kind == TokenKind::Equals)
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
        let missing = match expected {
            [one] => Some(*one),
            _ => None,
        };
        self.error_missing_token(message, expected_desc, missing);
    }

    fn error_at(&mut self, span: Span, message: impl Into<Message>, hint: impl Into<Message>) {
        self.errors.push(ParseError {
            message: message.into(),
            hint: hint.into(),
            span,
            fix: None,
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
            RecoveryLevel::Member => &[
                TokenKind::Fn,
                TokenKind::Prop,
                TokenKind::Type,
                TokenKind::CloseBrace,
            ],
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
        // `Self::new`, `Self::Item`: `Self` can start one.
        let first = match self.at(TokenKind::SelfType) {
            true => {
                let span = self.current_span();
                self.advance();
                Ident {
                    name: som_lang::SELF_TYPE.into(),
                    span,
                }
            }
            false => self.parse_ident(expected.clone())?,
        };
        let mut segments = vec![first];
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
            let before = self.pos;
            items.push(parse_item(self)?);
            // An item that took nothing (an error, reported) would be read again, forever:
            // `foo(i32)`, whose `i32` can start a list item but not an expression.
            if self.pos == before {
                return None;
            }
            if !self.list_continues(close) {
                break;
            }
        }
        Some(items)
    }

    /// After an item of a list ending in `close`: whether another follows, past its `,`. One
    /// that starts without the `,` before it, `[1, 2 3]`, is reported, with a fix putting the
    /// `,` in, and read as the next item.
    fn list_continues(&mut self, close: TokenKind) -> bool {
        if self.eat(TokenKind::Comma) {
            return true;
        }
        // `{ a: i32; b: i32 }`: another language's separator, changed and read as `,`.
        if self.at(TokenKind::Semicolon)
            && self.peek_nth(1) != close
            && can_start_item(self.peek_nth(1))
        {
            let span = self.current_span();
            if !self.in_recovery {
                self.errors.push(ParseError {
                    message: msg!["items are separated with ", code(","), ", not ", code(";")],
                    hint: msg!["expected ", code(",")],
                    fix: Some(Fix::replace(
                        msg!["change to ", code(",")],
                        span.clone(),
                        ",",
                    )),
                    span,
                });
            }
            self.advance();
            return true;
        }
        if self.at(close) || !can_start_item(self.peek()) {
            return false;
        }
        self.error_missing_token(
            msg!["missing ", TokenKind::Comma.part()],
            "a comma between the items",
            Some(TokenKind::Comma),
        );
        self.in_recovery = false;
        true
    }
}

/// Whether an item of a list (an expression, a field, a parameter, a type) can start with
/// `kind`: what tells a missing `,` from a missing closing bracket.
fn can_start_item(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::Text
            | TokenKind::True
            | TokenKind::False
            | TokenKind::None
            | TokenKind::SelfValue
            | TokenKind::SelfType
            | TokenKind::Mut
            | TokenKind::OpenParen
            | TokenKind::OpenBracket
            | TokenKind::Minus
            | TokenKind::Ampersand
            | TokenKind::Star
            | TokenKind::Bang
            | TokenKind::Tilde
            | TokenKind::Pub
            | TokenKind::Mod
    ) || ty::primitive(kind).is_some()
}

pub(crate) fn is_trivia(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Whitespace | TokenKind::Comment | TokenKind::ForeignComment
    )
}
