//! Bodies written without braces: `then … end`, `do … end`, Python's `:` and indentation.

use som_ast::{Span, Stmt};
use som_base::foreign::{self, Foreign};
use som_base::message::code;
use som_base::{Fix, msg};
use som_lexer::TokenKind;

use crate::report::RecoveryLevel;
use crate::stmt::BlockParts;
use crate::{Parser, StmtOrExpr};

/// What the `BLOCK` table's words stand for.
const OPENS: &str = "{";
const CLOSES: &str = "}";

impl Parser<'_> {
    /// A body as another language writes it, at where its `{` would be: `then … end` or
    /// `do … end` (Lua, Ruby), `:` and an indented block (Python), or, after a header, lines
    /// up to an `end` (Ruby's `def`, Lua's `function` and `else`). Reported, with a fix
    /// putting braces in (and the `;`s it's missing), and read.
    pub(crate) fn foreign_body(&mut self) -> Option<BlockParts> {
        let header_line = self.previous_span().end.line;
        let indent = self.indent_of(header_line);
        if let Some(found) = self.block_word().filter(|found| found.som == OPENS) {
            let opener = self.current_span();
            self.advance();
            return Some(self.keyword_body(found, Some(opener)));
        }
        let next_line = self.span_ahead(1).start.line > self.current_span().end.line;
        if self.at(TokenKind::Colon) && next_line {
            let found = foreign::BLOCK.find(":")?;
            let opener = self.current_span();
            self.advance();
            return Some(self.indented_body(found, opener, indent));
        }
        // Lines up to an `end` lined up with the header: the body, without anything opening it.
        let on_next_line = self.current_span().start.line > header_line;
        if on_next_line && self.end_ahead(indent) {
            let found = foreign::BLOCK.find("end")?;
            return Some(self.keyword_body(found, None));
        }
        None
    }

    /// The `BLOCK` word here, `then`, `do` or `end`, if it's one.
    fn block_word(&self) -> Option<&'static Foreign> {
        self.ident_ahead(0)
            .and_then(|word| foreign::BLOCK.find(word))
    }

    fn at_end_word(&self) -> bool {
        self.block_word().is_some_and(|found| found.som == CLOSES)
    }

    /// At what ends a body without braces: `end`, or an `else` (or `elif`, `elseif`)
    /// continuing an `if`.
    fn at_foreign_body_end(&self) -> bool {
        self.at(TokenKind::Else) || self.at_end_word() || self.foreign_else_if().is_some()
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
            .filter(|t| t.kind == TokenKind::Ident)
            .filter(|t| {
                foreign::BLOCK
                    .find(&t.text)
                    .is_some_and(|f| f.som == CLOSES)
            })
            .any(|t| t.span.start.col == indent + 1)
    }

    /// Statements up to `end`, or up to an `else` continuing an `if`. `opener` is `then` or
    /// `do`, changed to `{`; without one, a `{` goes after the header.
    fn keyword_body(&mut self, found: &'static Foreign, opener: Option<Span>) -> BlockParts {
        let header_end = self.previous_span().after();
        let marked = self.ctx.foreign_semicolons.len();
        let parts = self.in_foreign_body(|p| p.foreign_statements(|p| p.at_foreign_body_end()));
        let mut edits = match &opener {
            Some(opener) => vec![(opener.clone(), OPENS.to_string())],
            None => vec![(header_end, format!(" {OPENS}"))],
        };
        let close = self.current_span();
        if self.at_end_word() {
            edits.push((close.clone(), CLOSES.to_string()));
            self.advance();
        } else {
            edits.push((close.before(), format!("{CLOSES} ")));
        }
        edits.extend(self.semicolon_edits(marked));
        self.report_braces(opener.unwrap_or(close), found, edits);
        parts
    }

    /// Statements indented past `indent`, after Python's `:`.
    fn indented_body(&mut self, found: &'static Foreign, colon: Span, indent: usize) -> BlockParts {
        let marked = self.ctx.foreign_semicolons.len();
        let parts = self.in_foreign_body(|p| {
            p.foreign_statements(|p| p.current_span().start.col <= indent + 1)
        });
        let mut edits = vec![(colon.clone(), format!(" {OPENS}"))];
        edits.extend(self.semicolon_edits(marked));
        let closing = format!("\n{}{CLOSES}", " ".repeat(indent));
        edits.push((self.previous_span().after(), closing));
        self.report_braces(colon, found, edits);
        parts
    }

    fn report_braces(&mut self, span: Span, found: &'static Foreign, edits: Vec<(Span, String)>) {
        let message = msg!["som puts a body in braces: ", code("{ … }")];
        let fix = Fix {
            title: msg!["put it in braces"],
            edits,
        };
        self.report_foreign(span, message, found, Some(fix));
    }

    /// The `;`s missing from statements read since `marked`, as edits putting them in.
    fn semicolon_edits(&mut self, marked: usize) -> Vec<(Span, String)> {
        (self.ctx.foreign_semicolons.drain(marked..))
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
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) => {
                    if self.at(TokenKind::Semicolon) {
                        self.advance();
                    } else {
                        let at = self.previous_span().after();
                        self.ctx.foreign_semicolons.push(at);
                    }
                    let span = self.ast.exprs.span(expr).clone();
                    stmts.push(self.ast.stmts.alloc(Stmt::Expr(expr), span));
                }
                StmtOrExpr::Error => self.recover(RecoveryLevel::Statement),
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
        if self.ctx.foreign_depth == 0 {
            return false;
        }
        if !(self.on_new_line() || self.at_foreign_body_end() || self.at_eof()) {
            return false;
        }
        let at = self.previous_span().after();
        self.ctx.foreign_semicolons.push(at);
        true
    }
}
