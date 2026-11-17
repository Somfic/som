//! `@where` and `@on`, said of the function below them.

use som_ast::{Ident, Where};
use som_base::message::code;
use som_base::msg;
use som_lang::Builtin;
use som_lexer::TokenKind;

use crate::Parser;
use crate::cursor::unquoted;
use crate::grammar::VISIBILITY;
use crate::report::RecoveryLevel;

impl Parser<'_> {
    pub(crate) fn at_where(&self) -> bool {
        self.at_builtin(Builtin::Where) || self.at_builtin(Builtin::On)
    }

    /// `@where i < self.len`, `@on fail`, each line of them: kept for the function after
    /// them, with the `///` comment above them.
    pub(crate) fn parse_wheres(&mut self) {
        if self.wheres.is_empty() {
            self.wheres.doc = self.doc_comment();
            self.wheres.start = Some(self.current_span());
        }
        while self.at_where() {
            let read = match self.at_builtin(Builtin::On) {
                true => self.parse_on(),
                false => self.parse_where(),
            };
            if read.is_none() {
                self.recover(RecoveryLevel::Member);
                return;
            }
        }
        if !self.at_function() {
            let span = (self.wheres.start.take()).unwrap_or_else(|| self.current_span());
            self.wheres = Default::default();
            self.error_at(
                span,
                msg![code("@where"), " and ", code("@on"), " are for a function"],
                msg!["put it on the line above a ", code("fn")],
            );
        }
    }

    /// `@on fail`: the event's name.
    fn parse_on(&mut self) -> Option<()> {
        self.advance();
        let event = self.parse_ident("an event, like `fail`")?;
        self.wheres.listens.push(event);
        Some(())
    }

    /// `@where condition`, and after a `,`, what it means, for error messages.
    fn parse_where(&mut self) -> Option<()> {
        self.advance();
        let condition = self.parse_expr()?;
        let reason = match self.eat(TokenKind::Comma) && self.at(TokenKind::Text) {
            true => {
                let text = self.peek_token().text.clone();
                self.advance();
                Some(unquoted(&text).replace("\\\"", "\"").into())
            }
            false => None,
        };
        self.wheres.conditions.push(Where { condition, reason });
        Some(())
    }

    fn at_function(&self) -> bool {
        let mut at = 0;
        while VISIBILITY.contains(&self.peek_nth(at)) {
            at += 1;
        }
        matches!(self.peek_nth(at), TokenKind::Fn | TokenKind::Prop)
    }

    /// The `///` comment, `@where` conditions and `@on` events of the function starting here.
    pub(crate) fn take_wheres(&mut self) -> (Option<String>, Vec<Where>, Vec<Ident>) {
        let wheres = std::mem::take(&mut self.wheres);
        let doc = match wheres.doc {
            Some(doc) => Some(doc),
            None if !wheres.conditions.is_empty() || !wheres.listens.is_empty() => None,
            None => self.doc_comment(),
        };
        (doc, wheres.conditions, wheres.listens)
    }
}
