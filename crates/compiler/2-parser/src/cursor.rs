use som_ast::{Ident, Path, Span, Visibility};
use som_lang::Builtin;
use som_lexer::{Token, TokenKind};

use crate::Parser;

impl Parser<'_> {
    pub(crate) fn peek(&self) -> TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::Eof)
    }

    pub(crate) fn peek_token(&self) -> &Token {
        &self.tokens[self.pos]
    }

    /// The tokens from the current one on, without whitespace and comments.
    pub(crate) fn tokens_ahead(&self) -> impl Iterator<Item = &Token> {
        self.tokens[self.pos..]
            .iter()
            .filter(|t| !t.kind.is_trivia())
    }

    /// The nth token ahead (0 = current), whitespace and comments left out.
    pub(crate) fn ahead(&self, n: usize) -> Option<&Token> {
        self.tokens_ahead().nth(n)
    }

    pub(crate) fn peek_nth(&self, n: usize) -> TokenKind {
        self.ahead(n).map_or(TokenKind::Eof, |t| t.kind)
    }

    /// The name the nth token ahead is, if it's one: `Self` too.
    pub(crate) fn ident_ahead(&self, n: usize) -> Option<&str> {
        self.ahead(n)
            .filter(|t| matches!(t.kind, TokenKind::Ident | TokenKind::SelfType))
            .map(|t| &*t.text)
    }

    pub(crate) fn span_ahead(&self, n: usize) -> Span {
        self.ahead(n)
            .map_or_else(|| self.current_span(), |t| t.span.clone())
    }

    pub(crate) fn builtin_ahead(&self, n: usize) -> Option<Builtin> {
        self.ahead(n)
            .filter(|t| t.kind == TokenKind::Builtin)
            .and_then(|t| Builtin::from_name(&t.text))
    }

    pub(crate) fn self_ahead(&self, n: usize) -> bool {
        self.peek_nth(n) == TokenKind::SelfValue
    }

    /// Whether a `=` comes later in this statement, on this line.
    pub(crate) fn equals_ahead(&self) -> bool {
        let line = self.current_span().start.line;
        self.tokens_ahead()
            .take_while(|t| t.span.start.line == line && t.kind != TokenKind::Semicolon)
            .any(|t| t.kind == TokenKind::Equals)
    }

    pub(crate) fn at(&self, kind: TokenKind) -> bool {
        self.peek() == kind
    }

    pub(crate) fn at_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.contains(&self.peek())
    }

    pub(crate) fn at_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }

    pub(crate) fn at_builtin(&self, builtin: Builtin) -> bool {
        self.at(TokenKind::Builtin) && Builtin::from_name(&self.peek_token().text) == Some(builtin)
    }

    /// Whether the current token starts a later line than the one before it ends on.
    pub(crate) fn on_new_line(&self) -> bool {
        self.current_span().start.line > self.previous_span().end.line
    }

    pub(crate) fn current_span(&self) -> Span {
        self.peek_token().span.clone()
    }

    pub(crate) fn previous_span(&self) -> Span {
        let mut pos = self.pos.saturating_sub(1);
        while pos > 0 && self.tokens[pos].kind.is_trivia() {
            pos -= 1;
        }
        self.tokens[pos].span.clone()
    }

    /// From `start` up to the end of the last token taken.
    pub(crate) fn span_from(&self, start: &Span) -> Span {
        start.merge(&self.previous_span())
    }

    pub(crate) fn advance(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
        self.skip_trivia();
    }

    pub(crate) fn advance_by(&mut self, n: usize) {
        for _ in 0..n {
            self.advance();
        }
    }

    pub(crate) fn skip_trivia(&mut self) {
        while self.peek().is_trivia() {
            self.pos += 1;
        }
    }

    pub(crate) fn eat(&mut self, kind: TokenKind) -> bool {
        if !self.at(kind) {
            return false;
        }
        self.advance();
        self.in_recovery = false;
        true
    }

    pub(crate) fn expect(&mut self, kind: TokenKind, expected: impl Into<String>) -> Option<Span> {
        let span = self.current_span();
        if self.eat(kind) {
            return Some(span);
        }
        self.error_missing_token(
            som_base::msg!["missing ", kind.part()],
            expected,
            Some(kind),
        );
        None
    }

    /// The `///` comment right above the current token: consecutive `///` lines, joined.
    /// A blank line or an ordinary comment in between ends it.
    pub(crate) fn doc_comment(&self) -> Option<String> {
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

    pub(crate) fn parse_visibility(&mut self) -> Visibility {
        // `public static void main`: reported, and `public` read as the `pub` it means.
        let public = self.eat_foreign_modifiers();
        if public {
            self.eat(TokenKind::Pub);
            Visibility::Public
        } else if self.eat(TokenKind::Pub) {
            Visibility::Public
        } else if self.eat(TokenKind::Mod) {
            Visibility::Module
        } else {
            Visibility::Private
        }
    }

    pub(crate) fn parse_ident(&mut self, expected: impl Into<String>) -> Option<Ident> {
        if !self.at(TokenKind::Ident) {
            self.error_expected(&[TokenKind::Ident], expected);
            return None;
        }
        let token = self.peek_token();
        let ident = Ident {
            name: token.text.clone(),
            span: token.span.clone(),
        };
        self.advance();
        Some(ident)
    }

    /// A keyword taken as a name: `self` or `Self`.
    pub(crate) fn parse_keyword_name(&mut self, name: &str) -> Ident {
        let span = self.current_span();
        self.advance();
        Ident {
            name: name.into(),
            span,
        }
    }

    pub(crate) fn parse_self(&mut self) -> Ident {
        self.parse_keyword_name(som_lang::SELF_VALUE)
    }

    /// `name`, `a::b`, or one starting with `Self`: `Self::new`.
    pub(crate) fn parse_path(&mut self, expected: impl Into<String>) -> Option<Path> {
        let expected = expected.into();
        let first = match self.at(TokenKind::SelfType) {
            true => self.parse_keyword_name(som_lang::SELF_TYPE),
            false => self.parse_ident(expected.clone())?,
        };
        let mut segments = vec![first];
        while self.eat(TokenKind::DoubleColon) {
            segments.push(self.parse_ident(expected.clone())?);
        }
        Some(Path { segments })
    }

    /// `item, item, item` up to (not including) `close`, allowing a trailing comma.
    pub(crate) fn parse_comma_list<T>(
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

    /// Skips from an opening bracket `open` to its `close`, with what's nested in between.
    pub(crate) fn skip_balanced(&mut self, open: TokenKind, close: TokenKind) {
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

    /// Skips to the next `{`, and past the braces it opens.
    pub(crate) fn skip_to_braces(&mut self) {
        while !self.at_eof() && !self.at(TokenKind::OpenBrace) {
            self.advance();
        }
        if self.at(TokenKind::OpenBrace) {
            self.skip_balanced(TokenKind::OpenBrace, TokenKind::CloseBrace);
        }
    }
}

/// A text literal's text, without its quotes.
pub(crate) fn unquoted(text: &str) -> &str {
    &text[1..text.len() - 1]
}
