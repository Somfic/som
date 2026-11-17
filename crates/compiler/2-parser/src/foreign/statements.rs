use som_ast::{Expr, Id, Ident, Span, Stmt};
use som_base::foreign::{self, Foreign};
use som_base::message::code;
use som_base::{Fix, msg};
use som_lexer::TokenKind;

use crate::Parser;

const PASS: &str = "pass";
const SWITCH: &str = "switch";

impl Parser<'_> {
    /// A statement som has no counterpart for, at it: `pass`, `switch (x) { … }`. Reported,
    /// and read past; what it leaves, if anything.
    pub(crate) fn foreign_statement(&mut self) -> Option<Option<Id<Stmt>>> {
        let found = foreign::STATEMENT.find(self.ident_ahead(0)?)?;
        let start = self.current_span();
        match found.written {
            // `pass` alone on its line, or before `;` or `}`.
            PASS if matches!(
                self.peek_nth(1),
                TokenKind::Semicolon | TokenKind::CloseBrace | TokenKind::Eof
            ) || self.span_ahead(1).start.line > start.start.line =>
            {
                self.advance();
                self.eat(TokenKind::Semicolon);
                let span = self.span_from(&start);
                let message = msg![
                    "som has no ",
                    code(PASS),
                    ": a body with nothing to do is just ",
                    code("{}")
                ];
                let fix = Fix::replace(msg!["remove ", code(PASS)], span.clone(), "");
                self.report_foreign(span, message, found, Some(fix));
                Some(None)
            }
            // `switch (x) {` or `switch x {`: its cases are skipped.
            SWITCH if matches!(self.peek_nth(1), TokenKind::OpenParen | TokenKind::Ident) => {
                self.advance();
                while !matches!(self.peek(), TokenKind::OpenBrace | TokenKind::Eof) {
                    self.advance();
                }
                let header = self.span_from(&start);
                self.skip_braces();
                let message = msg![
                    "som has no ",
                    code(SWITCH),
                    ": it's ",
                    code("match x { 1 => …, _ => … }")
                ];
                self.report_foreign(header, message, found, None);
                Some(None)
            }
            _ => None,
        }
    }

    /// Skips a `{ … }`, with what's nested in it, from its `{`.
    pub(crate) fn skip_braces(&mut self) {
        let mut depth = 0usize;
        while !self.at_eof() {
            match self.peek() {
                TokenKind::OpenBrace => depth += 1,
                TokenKind::CloseBrace => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.advance();
                        return;
                    }
                }
                _ => {}
            }
            self.advance();
        }
    }

    /// An anonymous function, which som doesn't have, at it: `lambda x: …`,
    /// `function(a) { … }`, `fun x -> …`, `x => …`. Reported; then nothing more's read of
    /// the expression. Whether it was there.
    pub(crate) fn foreign_lambda(&mut self) -> bool {
        let word = self.ident_ahead(0);
        let written = match (word, self.peek_nth(1), self.peek_nth(2)) {
            (Some("lambda"), TokenKind::Ident | TokenKind::Colon, _) => "lambda",
            (Some("function"), TokenKind::OpenParen, _) => "function",
            (Some("func"), TokenKind::OpenParen, _) => "func",
            (Some("fun"), TokenKind::Ident, TokenKind::Arrow) => "->",
            (Some(_), TokenKind::FatArrow, _) => "=>",
            _ => return false,
        };
        self.report_lambda(written)
    }

    /// `(a, b) => …`, at the `(`: an anonymous function's parameters.
    pub(crate) fn foreign_lambda_params(&mut self) -> bool {
        let mut n = 1;
        let mut depth = 1;
        while depth > 0 {
            match self.peek_nth(n) {
                TokenKind::OpenParen => depth += 1,
                TokenKind::CloseParen => depth -= 1,
                TokenKind::Eof => return false,
                _ => {}
            }
            n += 1;
        }
        match self.peek_nth(n) {
            TokenKind::FatArrow => self.report_lambda("=>"),
            TokenKind::Arrow if self.peek_nth(n + 1) != TokenKind::OpenBrace => {
                self.report_lambda("->")
            }
            _ => false,
        }
    }

    fn report_lambda(&mut self, written: &str) -> bool {
        let Some(found) = foreign::LAMBDA.find(written) else {
            return false;
        };
        let span = self.current_span();
        let message = msg![
            "som has no anonymous functions: declare one with a name, ",
            code("fn name(a, b) = …"),
            ", and pass that"
        ];
        self.report_foreign(span, message, found, None);
        self.skip_expression();
        true
    }

    /// Skips the rest of an expression som can't read: up to the `;` ending its statement,
    /// or the bracket closing around it, with what's nested in between.
    fn skip_expression(&mut self) {
        let mut depth = 0usize;
        while !self.at_eof() {
            match self.peek() {
                TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace => depth += 1,
                TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                }
                TokenKind::Semicolon | TokenKind::Comma if depth == 0 => return,
                _ => {}
            }
            self.advance();
        }
    }

    /// `elif c`, `elsif c`, `elseif c`, at the word.
    pub(crate) fn foreign_else_if(&self) -> Option<&'static Foreign> {
        self.ident_ahead(0)
            .and_then(|word| foreign::ELSE_IF.find(word))
    }

    /// `elif c`, at the word: reported, changed to `else if`, and read past, leaving the
    /// condition.
    pub(crate) fn eat_foreign_else_if(&mut self) -> bool {
        let Some(found) = self.foreign_else_if() else {
            return false;
        };
        self.replace_word(found);
        true
    }

    /// `for (int i = 0; i < n; i++)`, at its `(`: reported, changed to `for i in 0..n` when
    /// it's a count like that. The loop's name and what it loops over, read as that range.
    pub(crate) fn c_for(&mut self) -> Option<(Ident, Id<Expr>)> {
        let found = foreign::OPERATOR.find("++")?;
        let open = self.current_span();
        self.advance();
        // `int i = 0` or `let i = 0`: a type or keyword, then the name.
        if self.peek_nth(1) == TokenKind::Ident {
            self.advance();
        }
        let name = self.parse_ident("the loop's variable")?;
        self.expect(TokenKind::Equals, "its first value")?;
        let from = self.nested(|p| p.parse_expr())?;
        self.expect(TokenKind::Semicolon, "`;` and when to stop")?;
        let counted = self.ident_ahead(0) == Some(&*name.name);
        self.advance();
        let inclusive = match self.peek() {
            TokenKind::LessThan => false,
            TokenKind::LessThanOrEqual => true,
            _ => return self.c_for_unknown(open, found),
        };
        self.advance();
        let to = self.nested(|p| p.parse_expr())?;
        self.expect(TokenKind::Semicolon, "`;` and how to step")?;
        let steps_one = self.ident_ahead(0) == Some(&*name.name)
            && self.peek_nth(1) == TokenKind::PlusPlus
            || self.at(TokenKind::PlusPlus) && self.ident_ahead(1) == Some(&*name.name);
        while !matches!(self.peek(), TokenKind::CloseParen | TokenKind::Eof) {
            self.advance();
        }
        self.expect(TokenKind::CloseParen, "`)` after the loop's header")?;
        let span = self.span_from(&open);
        let fix = (counted && steps_one).then(|| {
            let a = self.ast.exprs.span(from).get_text();
            let b = self.ast.exprs.span(to).get_text();
            let range = match inclusive {
                true => format!("{a}..{b} + 1"),
                false => format!("{a}..{b}"),
            };
            Fix::change_to(span.clone(), format!("{} in {range}", name.name))
        });
        self.report_c_for(span.clone(), found, fix);
        let range = Expr::Range {
            start: Some(from),
            end: Some(to),
        };
        Some((name, self.ast.exprs.alloc(range, span)))
    }

    /// A C `for` that isn't a plain count: reported, its header skipped.
    fn c_for_unknown(&mut self, open: Span, found: &'static Foreign) -> Option<(Ident, Id<Expr>)> {
        while !matches!(self.peek(), TokenKind::CloseParen | TokenKind::Eof) {
            self.advance();
        }
        self.eat(TokenKind::CloseParen);
        let span = self.span_from(&open);
        self.report_c_for(span, found, None);
        None
    }

    fn report_c_for(&mut self, span: Span, found: &'static Foreign, fix: Option<Fix>) {
        let message = msg![
            "som loops over a range: ",
            code("for i in 0..n"),
            ", or over a list: ",
            code("for x in xs")
        ];
        self.report_foreign(span, message, found, fix);
    }

    /// Lua's `for i = 1, n do`, at the `=` after the name: reported, changed to
    /// `for i in 1..n + 1`. What it loops over, read as that range.
    pub(crate) fn lua_for(&mut self, name: &Ident) -> Option<Id<Expr>> {
        let found = foreign::BLOCK.find("do")?;
        let start = name.span.clone();
        self.advance();
        let from = self.nested(|p| p.parse_expr())?;
        self.expect(TokenKind::Comma, "`,` and the last value")?;
        let to = self.parse_condition()?;
        let span = self.span_from(&start);
        let a = self.ast.exprs.span(from).get_text();
        let b = self.ast.exprs.span(to).get_text();
        let fixed = format!("{} in {a}..{b} + 1", name.name);
        let message = msg!["som loops over a range: ", code("for i in 1..n + 1")];
        let fix = Fix::change_to(span.clone(), fixed);
        self.report_foreign(span.clone(), message, found, Some(fix));
        let range = Expr::Range {
            start: Some(from),
            end: Some(to),
        };
        Some(self.ast.exprs.alloc(range, span))
    }
}
