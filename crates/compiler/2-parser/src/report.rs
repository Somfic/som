use som_ast::Span;
use som_base::message::code;
use som_base::{Fix, Message, msg};
use som_lexer::TokenKind;

use crate::grammar::{self, can_start_item};
use crate::{ParseError, Parser};

/// How far to skip ahead when recovering from a parse error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryLevel {
    Statement,
    Declaration,
    /// Inside an `impl` or `trait` block, whose `}` is left for the block.
    Member,
}

impl RecoveryLevel {
    fn sync_tokens(self) -> &'static [TokenKind] {
        match self {
            RecoveryLevel::Statement => grammar::STATEMENT_SYNC,
            RecoveryLevel::Member => grammar::MEMBER_SYNC,
            RecoveryLevel::Declaration => grammar::DECLARATION_SYNC,
        }
    }
}

impl Parser<'_> {
    pub(crate) fn error(
        &mut self,
        span: Span,
        message: impl Into<Message>,
        hint: impl Into<Message>,
        fix: Option<Fix>,
    ) {
        self.errors.push(ParseError {
            fix,
            ..ParseError::new(span, message, hint)
        });
    }

    /// An error, unless recovering from one.
    pub(crate) fn report(
        &mut self,
        span: Span,
        message: impl Into<Message>,
        hint: impl Into<Message>,
        fix: Option<Fix>,
    ) {
        if !self.in_recovery {
            self.error(span, message, hint, fix);
        }
    }

    pub(crate) fn error_at(
        &mut self,
        span: Span,
        message: impl Into<Message>,
        hint: impl Into<Message>,
    ) {
        self.error(span, message, hint, None);
    }

    pub(crate) fn error_missing(
        &mut self,
        message: impl Into<Message>,
        expected: impl Into<String>,
    ) {
        self.error_missing_token(message, expected, None);
    }

    /// Like `error_missing`, for a missing token `missing`: the fix puts it in, if it's one
    /// that's always written the same way.
    pub(crate) fn error_missing_token(
        &mut self,
        message: impl Into<Message>,
        expected: impl Into<String>,
        missing: Option<TokenKind>,
    ) {
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
                Some(kind) => self.missing_token_fix(kind, &found, &after),
                None => None,
            };
            self.error(span, message, msg!["expected ", expected.into()], fix);
        }
        self.in_recovery = true;
    }

    fn missing_token_fix(&self, kind: TokenKind, found: &Span, after: &Span) -> Option<Fix> {
        // `Circle(r) -> r`: the look-alike written in its place, changed.
        if let Some(text) = grammar::look_alike(kind, self.peek()) {
            return Some(Fix::replace(
                msg!["change ", code(self.peek().to_string()), " to ", code(text)],
                found.clone(),
                text,
            ));
        }
        if !self.insertion_fits(kind) {
            return None;
        }
        let text = grammar::insertion_text(kind)?;
        Some(Fix {
            title: msg!["add the missing ", code(text.trim())],
            edits: vec![(after.before(), text.to_string())],
        })
    }

    /// Whether putting a missing `kind` in before the current token makes sense of what
    /// follows. Not when what follows shows something else is wrong: `use std.io` isn't
    /// missing a `;` before `.`, nor `(a, b) => …` a `)` before `,`.
    fn insertion_fits(&self, kind: TokenKind) -> bool {
        let found = self.peek();
        if found == TokenKind::Eof || self.on_new_line() {
            return true;
        }
        match kind {
            TokenKind::Semicolon => self.at_item() || self.at_any(grammar::STATEMENT_START),
            TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace => {
                self.at_any(grammar::AFTER_CLOSER)
                    // `fn f(a: i32 -> i32`, but not `map(fun x -> x)`.
                    || (kind == TokenKind::CloseParen
                        && self.ctx.params
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
                !grammar::CLOSERS.contains(&self.peek_nth(1)) || !grammar::is_prefix_op(found)
            }
            _ => true,
        }
    }

    pub(crate) fn error_expected(
        &mut self,
        expected: &[TokenKind],
        expected_desc: impl Into<String>,
    ) {
        let mut message = match expected {
            [one] => msg!["expected ", one.part()],
            many => {
                let mut message = msg!["expected one of "];
                for (i, kind) in many.iter().enumerate() {
                    if i > 0 {
                        message.push(", ");
                    }
                    message.push(kind.part());
                }
                message
            }
        };
        message.push(", found ");
        message.push(self.peek().part());
        let missing = match expected {
            [one] => Some(*one),
            _ => None,
        };
        self.error_missing_token(message, expected_desc, missing);
    }

    /// After an item of a list ending in `close`: whether another follows, past its `,`. One
    /// that starts without the `,` before it, `[1, 2 3]`, is reported, with a fix putting the
    /// `,` in, and read as the next item.
    pub(crate) fn list_continues(&mut self, close: TokenKind) -> bool {
        if self.eat(TokenKind::Comma) {
            return true;
        }
        // `{ a: i32; b: i32 }`: another language's separator, changed and read as `,`.
        if self.at(TokenKind::Semicolon)
            && self.peek_nth(1) != close
            && can_start_item(self.peek_nth(1))
        {
            let span = self.current_span();
            self.report(
                span.clone(),
                msg!["items are separated with ", code(","), ", not ", code(";")],
                msg!["expected ", code(",")],
                Some(Fix::replace(msg!["change to ", code(",")], span, ",")),
            );
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

    pub(crate) fn recover(&mut self, level: RecoveryLevel) {
        while !self.at_eof() {
            if self.at_any(level.sync_tokens()) {
                self.eat(TokenKind::Semicolon);
                // The declaration loop doesn't take a stray `}`: past it, so it doesn't
                // loop forever.
                if level == RecoveryLevel::Declaration {
                    self.eat(TokenKind::CloseBrace);
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
}
