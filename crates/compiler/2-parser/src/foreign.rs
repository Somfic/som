//! Other languages' syntax, recognised where it's written: `def area(`, `if c then … end`,
//! `a === b`. Each is reported with what som writes instead (a fix where that's certain) and
//! then read as what it means, so one habit doesn't cascade into errors after it.
//!
//! What's foreign, and what som writes for it, is in [`som_base::foreign`]; this is where
//! the parser finds it.

use som_ast::{Expr, Id, Span, Stmt};
use som_base::foreign::{self, Foreign};
use som_base::message::{Message, code};
use som_base::{Fix, msg};
use som_lexer::TokenKind;

use crate::stmt::BlockParts;
use crate::{ParseError, Parser, is_trivia};

impl Parser<'_> {
    // --- Reporting ---

    /// Reports `foreign`, written at `span`, with what som writes for it.
    pub(crate) fn report_foreign(
        &mut self,
        span: Span,
        message: Message,
        foreign: &Foreign,
        fix: Option<Fix>,
    ) {
        if self.in_recovery {
            return;
        }
        self.errors.push(ParseError {
            message,
            hint: foreign.hint(),
            fix,
            span,
        });
    }

    /// Reports the current token, `foreign`'s word, as what som writes, `som`: "som writes
    /// `pub`, not `public`", changing it. Then reads past it.
    fn replace_word(&mut self, foreign: &Foreign, som: &str) {
        let span = self.current_span();
        let message = msg!["som writes ", code(som), ", not ", code(foreign.written)];
        let fix = Fix::replace(msg!["change to ", code(som)], span.clone(), som);
        self.report_foreign(span, message, foreign, Some(fix));
        self.advance();
    }

    /// `span`, with the whitespace after it up to the next token: what's removed when a word
    /// is, so `static fn` becomes `fn`.
    pub(crate) fn with_space_after(&self, span: &Span) -> Span {
        let next = (self.tokens.iter())
            .find(|t| !is_trivia(t.kind) && t.span.start_offset >= span.start_offset + span.length)
            .map_or_else(|| span.after(), |t| t.span.clone());
        Span {
            end: next.start,
            length: next.start_offset - span.start_offset,
            ..span.clone()
        }
    }

    /// Other languages' comments, `# note` and `-- note`: reported, each changed to `//`.
    pub(crate) fn report_foreign_comments(&mut self) {
        let comments: Vec<(Span, &'static Foreign)> = (self.tokens.iter())
            .filter(|t| t.kind == TokenKind::ForeignComment)
            .filter_map(|t| {
                let marker = if t.text.starts_with('#') { "#" } else { "--" };
                Some((t.span.clone(), foreign::comment(marker)?))
            })
            .collect();
        for (span, comment) in comments {
            let marker = Span {
                end: span.start,
                length: comment.written.len(),
                ..span.clone()
            };
            let message = msg!["som writes a comment with ", code("//")];
            let fix = Fix::replace(msg!["change to ", code("//")], marker.clone(), "//");
            self.report_foreign(marker, message, comment, Some(fix));
        }
    }

    // --- Declarations ---

    /// Another language's keyword `at` tokens ahead, found in `table`, followed by a name:
    /// `def area`, `class Point`.
    fn foreign_keyword(
        &self,
        at: usize,
        table: fn(&str) -> Option<&'static Foreign>,
    ) -> Option<&'static Foreign> {
        let found = table(self.ident_ahead(at)?)?;
        (self.peek_nth(at + 1) == TokenKind::Ident).then_some(found)
    }

    /// At another language's `fn`, `at` tokens ahead: `def area`.
    pub(crate) fn foreign_fn(&self, at: usize) -> bool {
        self.foreign_keyword(at, foreign::fn_word).is_some()
    }

    /// At another language's `struct`, `at` tokens ahead: `class Point`.
    pub(crate) fn foreign_struct(&self, at: usize) -> bool {
        self.foreign_keyword(at, foreign::struct_word).is_some()
    }

    /// At `fn`, or another language's word for it.
    pub(crate) fn at_fn(&self) -> bool {
        self.at(TokenKind::Fn) || self.foreign_fn(0)
    }

    pub(crate) fn expect_fn(&mut self, expected: &str) -> Option<()> {
        self.expect_keyword(TokenKind::Fn, foreign::fn_word, "functions", expected)
    }

    pub(crate) fn expect_struct(&mut self, expected: &str) -> Option<()> {
        self.expect_keyword(TokenKind::Struct, foreign::struct_word, "structs", expected)
    }

    /// Expects keyword `kind`, taking another language's word for it from `table` with an
    /// error: `def area` is read as the `fn area` it means. `declares` is what the keyword
    /// declares, for the message.
    fn expect_keyword(
        &mut self,
        kind: TokenKind,
        table: fn(&str) -> Option<&'static Foreign>,
        declares: &str,
        expected: &str,
    ) -> Option<()> {
        let Some(found) = self.foreign_keyword(0, table) else {
            return self.expect(kind, expected).map(|_| ());
        };
        let span = self.current_span();
        let som = kind.to_string();
        let message = msg![
            format!("som declares {declares} with "),
            code(&som),
            ", not ",
            code(found.written)
        ];
        let fix = Fix::replace(msg!["change to ", code(&som)], span.clone(), som);
        self.report_foreign(span, message, found, Some(fix));
        self.advance();
        Some(())
    }

    /// At another language's function, `at` tokens ahead, its return type first:
    /// `int sum(`, `String name(`.
    pub(crate) fn c_style_fn(&self, at: usize) -> bool {
        let ty = match self.peek_nth(at) {
            TokenKind::Ident => self.ident_ahead(at).is_some_and(|w| {
                foreign::fn_word(w).is_none() && foreign::struct_word(w).is_none()
            }),
            kind => crate::ty::primitive(kind).is_some(),
        };
        ty && self.peek_nth(at + 1) == TokenKind::Ident
            && self.peek_nth(at + 2) == TokenKind::OpenParen
    }

    /// At another language's `let`, followed by a name: `const name`, `var count`.
    pub(crate) fn foreign_let(&self) -> Option<&'static Foreign> {
        self.foreign_keyword(0, foreign::let_word)
    }

    /// At Go's `name := value`.
    pub(crate) fn go_define(&self) -> bool {
        self.at(TokenKind::Ident) && self.peek_nth(1) == TokenKind::ColonEquals
    }

    /// Another language's `let`, if it's there: reported, and read as the `let` it means.
    /// Whether it's mutable where it's from (`var`).
    pub(crate) fn eat_foreign_let(&mut self) -> Option<bool> {
        let found = self.foreign_let()?;
        let span = self.current_span();
        let message = msg![
            "som declares variables with ",
            code(found.som),
            ", not ",
            code(found.written)
        ];
        let fix = Fix::replace(msg!["change to ", code(found.som)], span.clone(), found.som);
        self.report_foreign(span, message, found, Some(fix));
        self.advance();
        Some(found.som == "let mut")
    }

    /// Go's `name :=`, at it: reported, changed to `let name =`. Reads the name and the
    /// `:=`, for what follows to be the value.
    pub(crate) fn eat_go_define(&mut self) -> Option<som_ast::Ident> {
        let found = foreign::operator(":=")?;
        let start = self.current_span();
        let name = self.parse_ident("a variable name")?;
        self.advance();
        let span = self.span_from(&start);
        let fixed = format!("let {} =", name.name);
        let message = msg!["som declares variables with ", code("let")];
        let fix = Fix::replace(msg!["change to ", code(&fixed)], span.clone(), fixed);
        self.report_foreign(span, message, found, Some(fix));
        Some(name)
    }

    /// Another language's `mut`, after `let`: `let mutable total`. Whether it was there.
    pub(crate) fn eat_foreign_mut(&mut self) -> bool {
        let Some(found) = self.foreign_keyword(0, foreign::mut_word) else {
            return false;
        };
        self.replace_word(found, found.som);
        true
    }

    /// Another language's modifier, `n` tokens ahead, before what it modifies: `public fn`,
    /// `static void main`.
    pub(crate) fn foreign_modifier(&self, n: usize) -> Option<&'static Foreign> {
        let found = foreign::modifier(self.ident_ahead(n)?)?;
        let next = self.peek_nth(n + 1);
        let modifies = matches!(
            next,
            TokenKind::Fn
                | TokenKind::Pub
                | TokenKind::Mod
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Trait
                | TokenKind::Prop
                | TokenKind::Type
                | TokenKind::Extern
        ) || next == TokenKind::Ident && self.peek_nth(n + 2) != TokenKind::Equals;
        modifies.then_some(found)
    }

    /// Other languages' modifiers, `public static`, each reported: `public` changed to the
    /// `pub` it means, the rest removed. Whether `public` was among them.
    pub(crate) fn eat_foreign_modifiers(&mut self) -> bool {
        let mut public = false;
        while let Some(found) = self.foreign_modifier(0) {
            if !found.som.is_empty() {
                public = true;
                self.replace_word(found, found.som);
                continue;
            }
            let span = self.current_span();
            let message = match found.written {
                "static" => msg![
                    "som has no ",
                    code("static"),
                    ": a function without ",
                    code("self"),
                    " is called on its type, as in ",
                    code("Type::name()")
                ],
                word => msg![
                    "som has no ",
                    code(word),
                    ": everything is private unless it's ",
                    code("pub")
                ],
            };
            let fix = Fix::replace(
                msg!["remove ", code(found.written)],
                self.with_space_after(&span),
                "",
            );
            self.report_foreign(span, message, found, Some(fix));
            self.advance();
        }
        public
    }

    /// `-> void`: another language's way of returning nothing, reported and read as nothing.
    pub(crate) fn void_return(&mut self) -> bool {
        if !self.at(TokenKind::Arrow) || self.ident_ahead(1) != Some("void") {
            return false;
        }
        let Some(found) = foreign::fn_word("void") else {
            return false;
        };
        // From the end of what's before it, so `f() -> void {` becomes `f() {`.
        let gap = self.previous_span().after();
        let start = self.current_span();
        self.advance();
        self.advance();
        let span = self.span_from(&start);
        let removed = Span {
            end: span.end,
            length: span.start_offset + span.length - gap.start_offset,
            ..gap
        };
        let message = msg![
            "som has no ",
            code("void"),
            ": a function returning nothing has no ",
            code("->")
        ];
        let fix = Fix::replace(msg!["remove ", code("-> void")], removed, "");
        self.report_foreign(span, message, found, Some(fix));
        true
    }

    /// `text.cstr` or `text.cstr()`, at `cstr`: a `string`'s text for C is its `c_str`, a
    /// property. Reported, with a fix, and read as it.
    pub(crate) fn c_str_misspelt(&mut self) {
        let start = self.current_span();
        let called =
            self.peek_nth(1) == TokenKind::OpenParen && self.peek_nth(2) == TokenKind::CloseParen;
        let span = if called {
            start.merge(&self.span_ahead(2))
        } else {
            start.clone()
        };
        if !self.in_recovery {
            self.errors.push(ParseError {
                message: msg![
                    "a ",
                    code("string"),
                    "'s text for C is its ",
                    code(".c_str")
                ],
                hint: msg![
                    code("cstr"),
                    " is the type; ",
                    code("c_str"),
                    " a property, read without parentheses"
                ],
                fix: Some(Fix::replace(
                    msg!["change to ", code("c_str")],
                    span,
                    "c_str",
                )),
                span: start,
            });
        }
        // Read on as the property: `c_str`, without the `()`.
        self.tokens[self.pos].kind = TokenKind::Ident;
        self.tokens[self.pos].text = "c_str".into();
        if called {
            let at = self.pos;
            let skipped: Vec<usize> = (at + 1..self.tokens.len())
                .filter(|&i| !is_trivia(self.tokens[i].kind))
                .take(2)
                .collect();
            for i in skipped {
                self.tokens[i].kind = TokenKind::Whitespace;
            }
        }
    }

    // --- Operators ---

    /// At another language's binary operator, `a && b`: reported, and made the token som
    /// writes, `and`, for the expression to read on as usual.
    pub(crate) fn rewrite_foreign_binary_op(&mut self) {
        let (written, som) = match self.peek() {
            TokenKind::AndAnd => ("&&", TokenKind::And),
            TokenKind::PipePipe => ("||", TokenKind::Or),
            TokenKind::TripleEquals => ("===", TokenKind::DoubleEquals),
            TokenKind::NotDoubleEquals => ("!==", TokenKind::NotEquals),
            TokenKind::TildeEquals => ("~=", TokenKind::NotEquals),
            _ => return,
        };
        let Some(found) = foreign::operator(written) else {
            return;
        };
        let span = self.current_span();
        let message = msg!["som writes ", code(found.som), ", not ", code(written)];
        let fix = Fix::replace(msg!["change to ", code(found.som)], span.clone(), found.som);
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
        if self.ident_ahead(0) != Some("not") || !starts_operand {
            return;
        }
        let Some(found) = foreign::operator("not") else {
            return;
        };
        let span = self.current_span();
        let message = msg!["som writes ", code("!"), ", not ", code("not")];
        let fix = Fix::replace(
            msg!["change to ", code("!")],
            self.with_space_after(&span),
            "!",
        );
        self.report_foreign(span, message, found, Some(fix));
        self.tokens[self.pos].kind = TokenKind::Bang;
    }

    /// `x++` or `x--` after `target`, ending a statement: reported, changed to `x = x + 1`
    /// or `x = x - 1`, and read past. Whether it was there.
    pub(crate) fn eat_foreign_step(&mut self, target: Id<Expr>) -> bool {
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
        let Some(found) = foreign::operator(written).filter(|_| ends) else {
            return false;
        };
        let start = self.current_span();
        for _ in 0..len {
            self.advance();
        }
        let span = self.span_from(&start);
        let target = self.ast.exprs.span(target).clone();
        let name = target.get_text();
        let fixed = format!("{name} = {name} {} 1", &written[..1]);
        let message = msg!["som has no ", code(written), ": write ", code(&fixed)];
        let fix = Fix::replace(msg!["change to ", code(&fixed)], target.merge(&span), fixed);
        self.report_foreign(span, message, found, Some(fix));
        true
    }

    /// `++x` starting a statement: reported, changed to `x = x + 1`. Whether it was there; the
    /// target is then read as usual.
    pub(crate) fn eat_foreign_prefix_step(&mut self) -> bool {
        if !self.at(TokenKind::PlusPlus) || self.peek_nth(1) != TokenKind::Ident {
            return false;
        }
        let Some(found) = foreign::operator("++") else {
            return false;
        };
        let start = self.current_span();
        let name = self.span_ahead(1);
        let span = start.merge(&name);
        let fixed = format!("{0} = {0} + 1", name.get_text());
        let message = msg!["som has no ", code("++"), ": write ", code(&fixed)];
        let fix = Fix::replace(msg!["change to ", code(&fixed)], span.clone(), fixed);
        self.report_foreign(span, message, found, Some(fix));
        self.advance();
        true
    }

    // --- Statements ---

    /// A statement som has no counterpart for, at it: `pass`, `switch (x) { … }`. Reported,
    /// and read past; what it leaves, if anything.
    pub(crate) fn foreign_statement(&mut self) -> Option<Option<Id<Stmt>>> {
        let word = self.ident_ahead(0)?;
        let found = foreign::statement(word)?;
        let start = self.current_span();
        match found.written {
            // `pass` alone on its line, or before `;` or `}`.
            "pass"
                if matches!(
                    self.peek_nth(1),
                    TokenKind::Semicolon | TokenKind::CloseBrace | TokenKind::Eof
                ) || self.span_ahead(1).start.line > start.start.line =>
            {
                self.advance();
                self.eat(TokenKind::Semicolon);
                let span = self.span_from(&start);
                let message = msg![
                    "som has no ",
                    code("pass"),
                    ": a body with nothing to do is just ",
                    code("{}")
                ];
                let fix = Fix::replace(msg!["remove ", code("pass")], span.clone(), "");
                self.report_foreign(span, message, found, Some(fix));
                Some(None)
            }
            // `switch (x) {` or `switch x {`: its cases are skipped.
            "switch" if matches!(self.peek_nth(1), TokenKind::OpenParen | TokenKind::Ident) => {
                self.advance();
                while !matches!(self.peek(), TokenKind::OpenBrace | TokenKind::Eof) {
                    self.advance();
                }
                let header = self.span_from(&start);
                self.skip_braces();
                let message = msg![
                    "som has no ",
                    code("switch"),
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

    /// `new Point(1, 2)`, at it: reported, and the `new` read past, leaving the call.
    pub(crate) fn eat_foreign_new(&mut self) -> bool {
        if self.ident_ahead(0) != Some("new") || self.peek_nth(1) != TokenKind::Ident {
            return false;
        }
        let Some(found) = foreign::statement("new") else {
            return false;
        };
        let span = self.current_span();
        let ty = self.ident_ahead(1).unwrap_or("Type").to_string();
        let message = msg![
            "som has no ",
            code("new"),
            ": make a struct with ",
            code(format!("{ty} {{ … }}")),
            ", or a function returning one, as in ",
            code(format!("{ty}::new(…)"))
        ];
        self.report_foreign(span, message, found, None);
        self.advance();
        true
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
        let Some(found) = foreign::lambda(written) else {
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

    /// `elif c`, `elsif c`, `elseif c`, at the word: reported, changed to `else if`, and read
    /// past, leaving the condition.
    pub(crate) fn eat_foreign_else_if(&mut self) -> bool {
        let Some(found) = self.ident_ahead(0).and_then(foreign::else_if) else {
            return false;
        };
        self.replace_word(found, found.som);
        true
    }

    // --- Loops ---

    /// `for (int i = 0; i < n; i++)`, at its `(`: reported, changed to `for i in 0..n` when
    /// it's a count like that. The loop's name and what it loops over, read as that range.
    pub(crate) fn c_for(&mut self) -> Option<(som_ast::Ident, Id<Expr>)> {
        let found = foreign::operator("++")?;
        let open = self.current_span();
        self.advance();
        // `int i = 0` or `let i = 0`: a type or keyword, then the name.
        if self.peek_nth(1) == TokenKind::Ident {
            self.advance();
        }
        let name = self.parse_ident("the loop's variable")?;
        self.expect(TokenKind::Equals, "its first value")?;
        let from = self.with_condition(false, |p| p.parse_expr())?;
        self.expect(TokenKind::Semicolon, "`;` and when to stop")?;
        let counted = self.ident_ahead(0) == Some(&*name.name);
        self.advance();
        let inclusive = match self.peek() {
            TokenKind::LessThan => false,
            TokenKind::LessThanOrEqual => true,
            _ => return self.c_for_unknown(open, found),
        };
        self.advance();
        let to = self.with_condition(false, |p| p.parse_expr())?;
        self.expect(TokenKind::Semicolon, "`;` and how to step")?;
        let steps_one = self.ident_ahead(0) == Some(&*name.name)
            && self.peek_nth(1) == TokenKind::PlusPlus
            || self.at(TokenKind::PlusPlus) && self.ident_ahead(1) == Some(&*name.name);
        while !matches!(self.peek(), TokenKind::CloseParen | TokenKind::Eof) {
            self.advance();
        }
        self.expect(TokenKind::CloseParen, "`)` after the loop's header")?;
        let span = self.span_from(&open);
        if !(counted && steps_one) {
            self.report_c_for(span.clone(), found, None);
        } else {
            let (a, b) = (
                self.ast.exprs.span(from).get_text(),
                self.ast.exprs.span(to).get_text(),
            );
            let range = if inclusive {
                format!("{a}..{b} + 1")
            } else {
                format!("{a}..{b}")
            };
            let fixed = format!("{} in {range}", name.name);
            let fix = Fix::replace(msg!["change to ", code(&fixed)], span.clone(), fixed);
            self.report_c_for(span.clone(), found, Some(fix));
        }
        let range = self.ast.exprs.alloc(
            Expr::Range {
                start: Some(from),
                end: Some(to),
            },
            span,
        );
        Some((name, range))
    }

    /// A C `for` that isn't a plain count: reported, its header skipped.
    fn c_for_unknown(
        &mut self,
        open: Span,
        found: &'static Foreign,
    ) -> Option<(som_ast::Ident, Id<Expr>)> {
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
    pub(crate) fn lua_for(&mut self, name: &som_ast::Ident) -> Option<Id<Expr>> {
        let found = foreign::block("do")?;
        let start = name.span.clone();
        self.advance();
        let from = self.with_condition(false, |p| p.parse_expr())?;
        self.expect(TokenKind::Comma, "`,` and the last value")?;
        let to = self.parse_condition()?;
        let span = self.span_from(&start);
        let (a, b) = (
            self.ast.exprs.span(from).get_text(),
            self.ast.exprs.span(to).get_text(),
        );
        let fixed = format!("{} in {a}..{b} + 1", name.name);
        let message = msg!["som loops over a range: ", code("for i in 1..n + 1")];
        let fix = Fix::replace(msg!["change to ", code(&fixed)], span.clone(), fixed);
        self.report_foreign(span.clone(), message, found, Some(fix));
        Some(self.ast.exprs.alloc(
            Expr::Range {
                start: Some(from),
                end: Some(to),
            },
            span,
        ))
    }

    // --- Bodies ---

    /// A body written as another language writes it, at where its `{` would be:
    /// `then … end` or `do … end` (Lua, Ruby), `:` and an indented block (Python), or, after
    /// a header, lines up to an `end` (Ruby's `def`, Lua's `function` and `else`).
    /// Reported, with a fix putting braces in (and the `;`s it's missing), and read.
    pub(crate) fn foreign_body(&mut self) -> Option<BlockParts> {
        let header_line = self.previous_span().end.line;
        let indent = self.indent_of(header_line);
        let word = self.ident_ahead(0);
        if matches!(word, Some("then" | "do")) {
            let found = foreign::block(word?)?;
            let opener = self.current_span();
            self.advance();
            return Some(self.keyword_body(found, Some(opener)));
        }
        let next_line = self.span_ahead(1).start.line > self.current_span().end.line;
        if self.at(TokenKind::Colon) && next_line {
            let found = foreign::block(":")?;
            let opener = self.current_span();
            self.advance();
            return Some(self.indented_body(found, opener, indent));
        }
        // Lines up to an `end` lined up with the header: the body, without anything opening it.
        let on_next_line = self.current_span().start.line > header_line;
        if on_next_line && self.end_ahead(indent) {
            let found = foreign::block("end")?;
            return Some(self.keyword_body(found, None));
        }
        None
    }

    /// How far line `line` is indented, in columns.
    fn indent_of(&self, line: usize) -> usize {
        let source = self.current_span().source.clone();
        let text = source
            .content()
            .lines()
            .nth(line.saturating_sub(1))
            .unwrap_or("");
        text.len() - text.trim_start().len()
    }

    /// Whether an `end` comes later, lined up at column `indent`.
    fn end_ahead(&self, indent: usize) -> bool {
        (self.tokens[self.pos..].iter())
            .filter(|t| t.kind == TokenKind::Ident && &*t.text == "end")
            .any(|t| t.span.start.col == indent + 1)
    }

    /// Statements up to `end`, or up to an `else` (or `elif`, `elseif`) continuing an `if`.
    /// `opener` is `then` or `do`, changed to `{`; without one, a `{` goes after the header.
    fn keyword_body(&mut self, found: &'static Foreign, opener: Option<Span>) -> BlockParts {
        let header_end = self.previous_span().after();
        let marked = self.foreign_semicolons.len();
        self.foreign_depth += 1;
        let parts = self.foreign_statements(|p| {
            p.at(TokenKind::Else)
                || matches!(p.ident_ahead(0), Some("end"))
                || p.ident_ahead(0).and_then(foreign::else_if).is_some()
        });
        self.foreign_depth -= 1;
        let mut edits = match &opener {
            Some(opener) => vec![(opener.clone(), "{".to_string())],
            None => vec![(header_end, " {".to_string())],
        };
        let close = self.current_span();
        if self.ident_ahead(0) == Some("end") {
            edits.push((close.clone(), "}".to_string()));
            self.advance();
        } else {
            edits.push((close.before(), "} ".to_string()));
        }
        edits.extend(self.semicolon_edits(marked));
        let span = opener.unwrap_or(close);
        let message = msg!["som puts a body in braces: ", code("{ … }")];
        let fix = Fix {
            title: msg!["put it in braces"],
            edits,
        };
        self.report_foreign(span, message, found, Some(fix));
        parts
    }

    /// Statements indented past `indent`, after Python's `:`.
    fn indented_body(&mut self, found: &'static Foreign, colon: Span, indent: usize) -> BlockParts {
        let marked = self.foreign_semicolons.len();
        self.foreign_depth += 1;
        let parts = self.foreign_statements(|p| p.current_span().start.col <= indent + 1);
        self.foreign_depth -= 1;
        let mut edits = vec![(colon.clone(), " {".to_string())];
        edits.extend(self.semicolon_edits(marked));
        let closing = format!("\n{}}}", " ".repeat(indent));
        edits.push((self.previous_span().after(), closing));
        let message = msg!["som puts a body in braces: ", code("{ … }")];
        let fix = Fix {
            title: msg!["put it in braces"],
            edits,
        };
        self.report_foreign(colon, message, found, Some(fix));
        parts
    }

    /// The `;`s missing from statements read since `marked`, as edits putting them in.
    fn semicolon_edits(&mut self, marked: usize) -> Vec<(Span, String)> {
        (self.foreign_semicolons.drain(marked..))
            .map(|at| (at, ";".to_string()))
            .collect()
    }

    /// Statements of a body without braces, until `ends` says it's ended (or the file does).
    /// A line's last statement, without its `;`, is kept; the `;` is noted for the fix.
    fn foreign_statements(&mut self, ends: impl Fn(&Self) -> bool) -> BlockParts {
        let start = self.current_span();
        let mut stmts = Vec::new();
        while !self.at_eof() && !ends(self) {
            let before = self.pos;
            match self.parse_stmt_or_expr() {
                crate::StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                crate::StmtOrExpr::Expr(expr) => {
                    if self.at(TokenKind::Semicolon) {
                        self.advance();
                    } else {
                        self.foreign_semicolons.push(self.previous_span().after());
                    }
                    let span = self.ast.exprs.span(expr).clone();
                    stmts.push(self.ast.stmts.alloc(Stmt::Expr(expr), span));
                }
                crate::StmtOrExpr::Error => self.recover(crate::RecoveryLevel::Statement),
            }
            if self.pos == before {
                self.advance();
            }
        }
        BlockParts::new(stmts, self.span_from(&start))
    }

    /// Whether a missing `;` at the current token is one a body without braces leaves out:
    /// the line ended, or the body did. Noted for its fix, rather than reported.
    pub(crate) fn foreign_semicolon(&mut self) -> bool {
        if self.foreign_depth == 0 {
            return false;
        }
        let ends_line = self.current_span().start.line > self.previous_span().end.line;
        let ends_body = self.at(TokenKind::Else)
            || matches!(self.ident_ahead(0), Some("end"))
            || self.ident_ahead(0).and_then(foreign::else_if).is_some()
            || self.at_eof();
        if !(ends_line || ends_body) {
            return false;
        }
        self.foreign_semicolons.push(self.previous_span().after());
        true
    }
}
