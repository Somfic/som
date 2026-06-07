use som_common::Span;

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

    pub(crate) fn next(&mut self) -> Token {
        let token = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    #[allow(dead_code)] // parser primitive; used once statements/`;` land
    pub(crate) fn eat(&mut self, kind: TokenKind) -> bool {
        if self.peek().kind == kind {
            self.next();
            true
        } else {
            false
        }
    }

    pub(crate) fn expect(&mut self, kind: TokenKind) -> Token {
        let token = self.peek().clone();
        if token.kind == kind {
            self.next()
        } else {
            self.diags.emit_error(
                token.span,
                format!("expected `{:?}`, found `{:?}`", kind, token.kind),
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
