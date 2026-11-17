//! Other languages' syntax, recognised where it's written: `def area(`, `if c then … end`,
//! `a === b`. Each is reported with what som writes instead (a fix where that's certain) and
//! then read as what it means, so one habit doesn't cascade into errors after it. What's
//! foreign, and what som writes for it, is in [`som_base::foreign`].

mod bodies;
mod operators;
mod statements;

use som_ast::Span;
use som_base::foreign::{self, Foreign, Table};
use som_base::message::{Message, code};
use som_base::{Fix, msg};
use som_lexer::TokenKind;

use crate::Parser;
use crate::ty::primitive;

const STATIC: &str = "static";
const VOID: &str = "void";
const NEW: &str = "new";
/// The standard library's property for a `string`'s text for C.
const C_STR: &str = "c_str";
/// Python's f-string, `f"total: {n}"`: som's text writes values in it as it is.
const F_STRING_PREFIXES: &[&str] = &["f", "F"];

impl Parser<'_> {
    /// Reports `foreign`, written at `span`, with what som writes for it.
    pub(crate) fn report_foreign(
        &mut self,
        span: Span,
        message: Message,
        foreign: &Foreign,
        fix: Option<Fix>,
    ) {
        self.report(span, message, foreign.hint(), fix);
    }

    /// Reports the current token, `foreign`'s word, as what som writes: "som writes `pub`,
    /// not `public`", changing it. Then reads past it.
    fn replace_word(&mut self, foreign: &Foreign) {
        let span = self.current_span();
        let message = msg![
            "som writes ",
            code(foreign.som),
            ", not ",
            code(foreign.written)
        ];
        let fix = Fix::change_to(span.clone(), foreign.som);
        self.report_foreign(span, message, foreign, Some(fix));
        self.advance();
    }

    /// `span`, with the whitespace after it up to the next token: what's removed when a word
    /// is, so `static fn` becomes `fn`.
    pub(crate) fn with_space_after(&self, span: &Span) -> Span {
        let next = (self.tokens.iter())
            .find(|t| !t.kind.is_trivia() && t.span.start_offset >= span.start_offset + span.length)
            .map_or_else(|| span.after(), |t| t.span.clone());
        Span {
            end: next.start,
            length: next.start_offset - span.start_offset,
            ..span.clone()
        }
    }

    /// `# note` and `-- note`: reported, each changed to `//`.
    pub(crate) fn report_foreign_comments(&mut self) {
        let comments: Vec<(Span, &'static Foreign)> = (self.tokens.iter())
            .filter(|t| t.kind == TokenKind::ForeignComment)
            .filter_map(|t| {
                let marker = if t.text.starts_with('#') { "#" } else { "--" };
                Some((t.span.clone(), foreign::COMMENT.find(marker)?))
            })
            .collect();
        for (span, comment) in comments {
            let marker = Span {
                end: span.start,
                length: comment.written.len(),
                ..span.clone()
            };
            let message = msg!["som writes a comment with ", code(comment.som)];
            let fix = Fix::change_to(marker.clone(), comment.som);
            self.report_foreign(marker, message, comment, Some(fix));
        }
    }

    /// Another language's keyword `at` tokens ahead, found in `table`, followed by a name:
    /// `def area`, `class Point`.
    pub(crate) fn foreign_keyword(&self, at: usize, table: &Table) -> Option<&'static Foreign> {
        let found = table.find(self.ident_ahead(at)?)?;
        (self.peek_nth(at + 1) == TokenKind::Ident).then_some(found)
    }

    /// `def area`.
    pub(crate) fn foreign_fn(&self, at: usize) -> bool {
        self.foreign_keyword(at, &foreign::FN).is_some()
    }

    /// `class Point`.
    pub(crate) fn foreign_struct(&self, at: usize) -> bool {
        self.foreign_keyword(at, &foreign::STRUCT).is_some()
    }

    /// At `fn`, or another language's word for it.
    pub(crate) fn at_fn(&self) -> bool {
        self.at(TokenKind::Fn) || self.foreign_fn(0)
    }

    pub(crate) fn expect_fn(&mut self, expected: &str) -> Option<()> {
        self.expect_keyword(TokenKind::Fn, &foreign::FN, "functions", expected)
    }

    pub(crate) fn expect_struct(&mut self, expected: &str) -> Option<()> {
        self.expect_keyword(TokenKind::Struct, &foreign::STRUCT, "structs", expected)
    }

    /// Expects keyword `kind`, taking another language's word for it from `table` with an
    /// error: `def area` is read as the `fn area` it means. `declares` is what the keyword
    /// declares, for the message.
    fn expect_keyword(
        &mut self,
        kind: TokenKind,
        table: &Table,
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
        let fix = Fix::change_to(span.clone(), som);
        self.report_foreign(span, message, found, Some(fix));
        self.advance();
        Some(())
    }

    /// At another language's function, `at` tokens ahead, its return type first:
    /// `int sum(`, `String name(`.
    pub(crate) fn c_style_fn(&self, at: usize) -> bool {
        let ty = match self.peek_nth(at) {
            TokenKind::Ident => self.ident_ahead(at).is_some_and(|w| {
                foreign::FN.find(w).is_none() && foreign::STRUCT.find(w).is_none()
            }),
            kind => primitive(kind).is_some(),
        };
        ty && self.peek_nth(at + 1) == TokenKind::Ident
            && self.peek_nth(at + 2) == TokenKind::OpenParen
    }

    /// `const name`, `var count`.
    pub(crate) fn foreign_let(&self) -> Option<&'static Foreign> {
        self.foreign_keyword(0, &foreign::LET)
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
        let fix = Fix::change_to(span.clone(), found.som);
        self.report_foreign(span, message, found, Some(fix));
        self.advance();
        Some(found.som == "let mut")
    }

    /// Go's `name :=`, at it: reported, changed to `let name =`. Reads the name and the
    /// `:=`, for what follows to be the value.
    pub(crate) fn eat_go_define(&mut self) -> Option<som_ast::Ident> {
        let found = foreign::OPERATOR.find(":=")?;
        let start = self.current_span();
        let name = self.parse_ident("a variable name")?;
        self.advance();
        let span = self.span_from(&start);
        let fixed = format!("let {} =", name.name);
        let message = msg!["som declares variables with ", code("let")];
        let fix = Fix::change_to(span.clone(), fixed);
        self.report_foreign(span, message, found, Some(fix));
        Some(name)
    }

    /// `let mutable total`, after `let`. Whether it was there.
    pub(crate) fn eat_foreign_mut(&mut self) -> bool {
        let Some(found) = self.foreign_keyword(0, &foreign::MUT) else {
            return false;
        };
        self.replace_word(found);
        true
    }

    /// Another language's modifier, `n` tokens ahead, before what it modifies: `public fn`,
    /// `static void main`.
    pub(crate) fn foreign_modifier(&self, n: usize) -> Option<&'static Foreign> {
        let found = foreign::MODIFIER.find(self.ident_ahead(n)?)?;
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

    /// `public static`, each reported: `public` changed to the `pub` it means, the rest
    /// removed. Whether `public` was among them.
    pub(crate) fn eat_foreign_modifiers(&mut self) -> bool {
        let mut public = false;
        while let Some(found) = self.foreign_modifier(0) {
            if !found.som.is_empty() {
                public = true;
                self.replace_word(found);
                continue;
            }
            let span = self.current_span();
            let message = match found.written {
                STATIC => msg![
                    "som has no ",
                    code(STATIC),
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
        if !self.at(TokenKind::Arrow) || self.ident_ahead(1) != Some(VOID) {
            return false;
        }
        let Some(found) = foreign::FN.find(VOID) else {
            return false;
        };
        // From the end of what's before it, so `f() -> void {` becomes `f() {`.
        let gap = self.previous_span().after();
        let start = self.current_span();
        self.advance_by(2);
        let span = self.span_from(&start);
        let removed = Span {
            end: span.end,
            length: span.start_offset + span.length - gap.start_offset,
            ..gap
        };
        let message = msg![
            "som has no ",
            code(VOID),
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
        let span = match called {
            true => start.merge(&self.span_ahead(2)),
            false => start.clone(),
        };
        self.report(
            start,
            msg![
                "a ",
                code("string"),
                "'s text for C is its ",
                code(".c_str")
            ],
            msg![
                code("cstr"),
                " is the type; ",
                code("c_str"),
                " a property, read without parentheses"
            ],
            Some(Fix::change_to(span, C_STR)),
        );
        // Read on as the property: `c_str`, without the `()`.
        self.tokens[self.pos].kind = TokenKind::Ident;
        self.tokens[self.pos].text = C_STR.into();
        if called {
            let skipped: Vec<usize> = (self.pos + 1..self.tokens.len())
                .filter(|&i| !self.tokens[i].kind.is_trivia())
                .take(2)
                .collect();
            for i in skipped {
                self.tokens[i].kind = TokenKind::Whitespace;
            }
        }
    }

    /// `new Point(1, 2)`, at it: reported, and the `new` read past, leaving the call.
    pub(crate) fn eat_foreign_new(&mut self) -> bool {
        if self.ident_ahead(0) != Some(NEW) || self.peek_nth(1) != TokenKind::Ident {
            return false;
        }
        let Some(found) = foreign::STATEMENT.find(NEW) else {
            return false;
        };
        let span = self.current_span();
        let ty = self.ident_ahead(1).unwrap_or("Type").to_string();
        let message = msg![
            "som has no ",
            code(NEW),
            ": make a struct with ",
            code(format!("{ty} {{ … }}")),
            ", or a function returning one, as in ",
            code(format!("{ty}::new(…)"))
        ];
        self.report_foreign(span, message, found, None);
        self.advance();
        true
    }

    /// `f"total: {n}"`, at the `f`: reported, and read past.
    pub(crate) fn eat_foreign_f_string(&mut self) {
        let prefixed = self.at(TokenKind::Ident)
            && F_STRING_PREFIXES.contains(&&*self.peek_token().text)
            && self.peek_nth(1) == TokenKind::Text
            && self.span_ahead(1).start_offset == self.current_span().start_offset + 1;
        if !prefixed {
            return;
        }
        let prefix = self.current_span();
        self.report(
            prefix.clone(),
            msg!["som's text has no ", code("f"), " in front"],
            msg!["any text writes the values in its ", code("{…}")],
            Some(Fix::replace(msg!["remove the ", code("f")], prefix, "")),
        );
        self.advance();
    }
}
