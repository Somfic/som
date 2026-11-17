use som_ast::{Expr, Id};
use som_base::foreign;
use som_base::message::code;
use som_base::{Fix, msg};
use som_lexer::TokenKind;

use crate::Parser;

/// Other languages' binary operators, as lexed, and the token som writes for each.
const BINARY: &[(TokenKind, TokenKind)] = &[
    (TokenKind::And, TokenKind::AndAnd),
    (TokenKind::Or, TokenKind::PipePipe),
    (TokenKind::TripleEquals, TokenKind::DoubleEquals),
    (TokenKind::NotDoubleEquals, TokenKind::NotEquals),
    (TokenKind::TildeEquals, TokenKind::NotEquals),
];

impl Parser<'_> {
    /// At another language's binary operator, `a and b`: reported, and made the token som
    /// writes, `&&`, for the expression to read on as usual.
    pub(crate) fn rewrite_foreign_binary_op(&mut self) {
        let Some(&(written, som)) = BINARY.iter().find(|(written, _)| self.at(*written)) else {
            return;
        };
        let written = written.to_string();
        let Some(found) = foreign::OPERATOR.find(&written) else {
            return;
        };
        let span = self.current_span();
        let message = msg!["som writes ", code(found.som), ", not ", code(&written)];
        let fix = Fix::change_to(span.clone(), found.som);
        self.report_foreign(span, message, found, Some(fix));
        self.tokens[self.pos].kind = som;
    }

    /// Python's `not x`, at it: reported, changed to `!`, and made a `!` for the expression
    /// to read on as usual.
    pub(crate) fn rewrite_foreign_not(&mut self) {
        let starts_operand = matches!(
            self.peek_nth(1),
            TokenKind::Ident
                | TokenKind::OpenParen
                | TokenKind::True
                | TokenKind::False
                | TokenKind::SelfValue
                | TokenKind::Int
        );
        let not = TokenKind::Bang.to_string();
        let Some(found) = (self.ident_ahead(0))
            .and_then(|word| foreign::OPERATOR.find(word))
            .filter(|found| found.som == not)
        else {
            return;
        };
        if !starts_operand {
            return;
        }
        let span = self.current_span();
        let message = msg![
            "som writes ",
            code(found.som),
            ", not ",
            code(found.written)
        ];
        let fix = Fix::change_to(self.with_space_after(&span), found.som);
        self.report_foreign(span, message, found, Some(fix));
        self.tokens[self.pos].kind = TokenKind::Bang;
    }

    /// `x++` or `x--` after `target`, ending a statement: reported, changed to `x = x + 1`
    /// or `x = x - 1`, and read past. Whether it was there.
    pub(crate) fn eat_foreign_step(&mut self, target: Id<Expr>) -> bool {
        // `--` is lexed as two minuses, right next to each other.
        let (written, len) = if self.at(TokenKind::PlusPlus) {
            ("++", 1)
        } else if self.at(TokenKind::Minus)
            && self.peek_nth(1) == TokenKind::Minus
            && self.span_ahead(1).start_offset == self.current_span().start_offset + 1
        {
            ("--", 2)
        } else {
            return false;
        };
        let ends = matches!(
            self.peek_nth(len),
            TokenKind::Semicolon | TokenKind::CloseParen | TokenKind::CloseBrace | TokenKind::Eof
        ) || self.span_ahead(len).start.line > self.current_span().start.line;
        let Some(found) = foreign::OPERATOR.find(written).filter(|_| ends) else {
            return false;
        };
        let start = self.current_span();
        self.advance_by(len);
        let span = self.span_from(&start);
        let target = self.ast.exprs.span(target).clone();
        let name = target.get_text();
        let fixed = format!("{name} = {name} {} 1", &written[..1]);
        let message = msg!["som has no ", code(written), ": write ", code(&fixed)];
        let fix = Fix::change_to(target.merge(&span), fixed);
        self.report_foreign(span, message, found, Some(fix));
        true
    }

    /// `++x` starting a statement: reported, changed to `x = x + 1`. Whether it was there; the
    /// target is then read as usual.
    pub(crate) fn eat_foreign_prefix_step(&mut self) -> bool {
        if !self.at(TokenKind::PlusPlus) || self.peek_nth(1) != TokenKind::Ident {
            return false;
        }
        let Some(found) = foreign::OPERATOR.find("++") else {
            return false;
        };
        let start = self.current_span();
        let name = self.span_ahead(1);
        let span = start.merge(&name);
        let fixed = format!("{0} = {0} + 1", name.get_text());
        let message = msg!["som has no ", code("++"), ": write ", code(&fixed)];
        let fix = Fix::change_to(span.clone(), fixed);
        self.report_foreign(span, message, found, Some(fix));
        self.advance();
        true
    }
}
