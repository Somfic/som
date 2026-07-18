use som_common::{Span, message};

use crate::{
    Parser,
    token::{Token, TokenKind},
};

impl Parser<'_> {
    pub(crate) fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .expect("token stream should always end with EOF")
    }

    pub(crate) fn peek_nth(&self, n: usize) -> &Token {
        self.tokens
            .get(self.pos + n)
            .or_else(|| self.tokens.last())
            .expect("token stream should always end with EOF")
    }

    pub(crate) fn next(&mut self) -> Token {
        let token = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    pub(crate) fn skip_layout(&mut self) {
        while matches!(
            self.peek().kind,
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.next();
        }
    }

    pub(crate) fn skip_newlines(&mut self) {
        while self.peek().kind == TokenKind::Newline {
            self.next();
        }
    }

    pub(crate) fn try_eat(&mut self, kind: TokenKind) -> Option<Token> {
        if self.peek().kind == kind {
            Some(self.next())
        } else {
            None
        }
    }

    pub(crate) fn expect(&mut self, kind: TokenKind) -> Token {
        let token = self.peek().clone();
        if token.kind == kind {
            self.next()
        } else {
            self.diags.emit_error(
                token.span,
                message!["expected ", kind, ", found ", token.kind],
            );
            // Synthetic token at the current position; don't advance.
            Token {
                kind,
                span: Span {
                    end: token.span.start,
                    ..token.span
                },
                text: "".into(),
            }
        }
    }
}
