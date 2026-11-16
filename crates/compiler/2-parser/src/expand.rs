//! `@for` over items: `@for T in [u8, u16] { impl Hash for T { … } }`. Each copy of the body
//! is the items with `T` written as that element, before anything else sees them: resolve and
//! the type checker see one `impl` for each type, written out.
//!
//! It's done on tokens, before parsing them: the tokens of a copy of the body are put where
//! the `@for` was, then parsed like anything else. Each copy's tokens keep where they're
//! written, so what's wrong in one is said where the body is.

use som_base::message::code;
use som_base::msg;
use som_lexer::{Token, TokenKind};

use crate::Parser;

impl Parser<'_> {
    /// At an `@for` whose body is items, not statements: `@for T in [u8, u16] { impl … }`.
    /// In an impl block, `members`, it's methods, or anything else a block has.
    pub(crate) fn at_item_for(&self, members: bool) -> bool {
        if !(self.at(TokenKind::Builtin) && &*self.peek_token().text == "@for") {
            return false;
        }
        if members {
            return true;
        }
        // Past `@for name in`, then what it loops over, to the body's `{`.
        let mut n = 3;
        let mut depth = 0;
        loop {
            match self.peek_nth(n) {
                TokenKind::OpenBracket | TokenKind::OpenParen => depth += 1,
                TokenKind::CloseBracket | TokenKind::CloseParen => depth -= 1,
                TokenKind::OpenBrace if depth == 0 => break,
                TokenKind::Eof | TokenKind::Semicolon => return false,
                _ => {}
            }
            n += 1;
        }
        self.peek_nth(n) == TokenKind::OpenBrace
            && matches!(
                self.peek_nth(n + 1),
                TokenKind::Fn
                    | TokenKind::Impl
                    | TokenKind::Struct
                    | TokenKind::Enum
                    | TokenKind::Trait
                    | TokenKind::Type
                    | TokenKind::Pub
                    | TokenKind::Mod
                    | TokenKind::Extern
                    | TokenKind::Use
                    | TokenKind::Builtin
                    | TokenKind::Prop
            )
    }

    /// Writes the `@for` here out: a copy of its body's tokens for each element, in its place.
    /// What's after it is then parsed as if they'd been written.
    pub(crate) fn expand_for(&mut self) {
        let start = self.pos;
        let keyword = self.current_span();
        self.advance();
        let Some(name) = self.parse_ident("a name for each element") else {
            self.skip_for(start);
            return;
        };
        if self.expect(TokenKind::In, "`in` and what to loop over").is_none() {
            self.skip_for(start);
            return;
        }
        // Each element's tokens.
        let elements: Vec<Vec<Token>> = if self.at(TokenKind::OpenBracket) {
            self.advance();
            let mut elements = vec![Vec::new()];
            let mut depth = 0;
            while !self.at_eof() {
                let token = self.peek_token().clone();
                match token.kind {
                    TokenKind::CloseBracket if depth == 0 => break,
                    TokenKind::Comma if depth == 0 => {
                        elements.push(Vec::new());
                        self.advance();
                        continue;
                    }
                    TokenKind::OpenBracket
                    | TokenKind::OpenParen
                    | TokenKind::OpenBrace
                    | TokenKind::LessThan => depth += 1,
                    TokenKind::CloseBracket
                    | TokenKind::CloseParen
                    | TokenKind::CloseBrace
                    | TokenKind::GreaterThan => depth -= 1,
                    _ => {}
                }
                elements.last_mut().expect("one at least").push(token);
                self.advance();
            }
            self.expect(TokenKind::CloseBracket, "`]` after the elements");
            // `[a, b,]`: a comma after the last.
            elements.into_iter().filter(|e| !e.is_empty()).collect()
        } else if let Some(range) = self.int_range() {
            range
        } else {
            self.error_at(
                keyword,
                msg![
                    "an ",
                    code("@for"),
                    " around items goes through a list written out, or a range of numbers"
                ],
                msg!["write it ", code("@for T in [u8, u16] { … }"), " or ", code("@for i in 0..4 { … }")],
            );
            self.skip_for(start);
            return;
        };
        if self.expect(TokenKind::OpenBrace, "`{` and the items").is_none() {
            self.skip_for(start);
            return;
        }
        // The body's tokens, trivia and all, up to its `}`.
        let body_start = self.pos;
        let mut depth = 1;
        let mut end = body_start;
        while end < self.tokens.len() && self.tokens[end].kind != TokenKind::Eof {
            match self.tokens[end].kind {
                TokenKind::OpenBrace => depth += 1,
                TokenKind::CloseBrace => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            end += 1;
        }
        let body = self.tokens[body_start..end].to_vec();
        let past = (end + 1).min(self.tokens.len() - 1);
        let mut written = Vec::new();
        for element in &elements {
            for token in &body {
                match token.kind == TokenKind::Ident && *token.text == *name.name {
                    true => written.extend(element.iter().cloned()),
                    false => written.push(token.clone()),
                }
            }
        }
        self.tokens.splice(start..past, written);
        self.pos = start;
        self.skip_trivia();
    }

    /// `0..4`, of literals: each number, as a token.
    fn int_range(&mut self) -> Option<Vec<Vec<Token>>> {
        if self.peek_nth(0) != TokenKind::Int
            || self.peek_nth(1) != TokenKind::DotDot
            || self.peek_nth(2) != TokenKind::Int
        {
            return None;
        }
        let first = self.peek_token().clone();
        self.advance();
        self.advance();
        let last = self.peek_token().clone();
        self.advance();
        let number = |t: &Token| t.text.replace('_', "").parse::<i64>().ok();
        let (from, to) = (number(&first)?, number(&last)?);
        let span = first.span.clone();
        Some(
            (from..to)
                .map(|n| {
                    vec![Token {
                        kind: TokenKind::Int,
                        text: n.to_string().into(),
                        span: span.clone(),
                    }]
                })
                .collect(),
        )
    }

    /// Past an `@for` that couldn't be read, from `start`: to the end of its body.
    fn skip_for(&mut self, start: usize) {
        if self.pos == start {
            self.advance();
        }
        while !self.at_eof() && !self.at(TokenKind::OpenBrace) {
            self.advance();
        }
        if self.at(TokenKind::OpenBrace) {
            self.skip_balanced(TokenKind::OpenBrace, TokenKind::CloseBrace);
        }
    }
}
