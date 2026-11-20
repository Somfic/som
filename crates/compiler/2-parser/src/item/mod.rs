//! Items: what a file declares, and the loose statements of an implicit `fn main()`.

mod attrs;
mod externs;
mod func;
mod impl_trait;
mod struct_enum;

use som_ast::{Alias, Const, Expr, Func, Id, Ident, Item, Use, Visibility};
use som_base::message::code;
use som_base::{Fix, foreign, msg};
use som_lang::Builtin;
use som_lexer::TokenKind;

use crate::grammar::{ITEM_KEYWORDS, VISIBILITY};
use crate::report::RecoveryLevel;
use crate::{Parser, StmtOrExpr};

impl Parser<'_> {
    pub(crate) fn at_item(&self) -> bool {
        // `pub fn …`: who can see it, then the item. Other languages' `public` and
        // `static` too, reported when the item's read.
        let mut at = 0;
        while VISIBILITY.contains(&self.peek_nth(at)) || self.foreign_modifier(at).is_some() {
            at += 1;
        }
        self.at_where()
            || ITEM_KEYWORDS.contains(&self.peek_nth(at))
            || self.at_const(at)
            || self.at_check(at)
            || self.foreign_fn(at)
            || self.c_style_fn(at)
            || self.foreign_struct(at)
    }

    /// A file is a list of items, optionally followed by loose statements that form the body
    /// of an implicit `fn main()`.
    pub(crate) fn parse_program(&mut self) {
        while !self.at_eof() {
            if self.at_item_for(false) {
                self.expand_for();
            } else if self.at_item() {
                self.parse_item();
            } else {
                break;
            }
        }
        if !self.at_eof() {
            self.parse_implicit_main();
        }
    }

    fn parse_item(&mut self) {
        self.ctx.impl_block = false;
        if self.at_where() {
            self.parse_wheres();
            return;
        }
        let at = self.current_span();
        self.visibility = self.parse_visibility();
        self.check_container_visibility(at);
        let item = match self.peek() {
            TokenKind::Fn => self.parse_function().map(Item::Func),
            TokenKind::Ident if self.foreign_fn(0) => self.parse_function().map(Item::Func),
            _ if self.c_style_fn(0) => self.parse_function().map(Item::Func),
            TokenKind::Ident if self.foreign_struct(0) => self.parse_struct().map(Item::Struct),
            TokenKind::Extern if self.peek_nth(1) == TokenKind::Struct => {
                self.parse_struct().map(Item::Struct)
            }
            TokenKind::Extern => self.parse_extern().map(Item::Extern),
            TokenKind::Struct => self.parse_struct().map(Item::Struct),
            TokenKind::Enum => self.parse_enum().map(Item::Struct),
            TokenKind::Trait => self.parse_trait().map(Item::Trait),
            TokenKind::Use if self.peek_nth(1) == TokenKind::Extern => {
                self.parse_use_extern().map(Item::Extern)
            }
            TokenKind::Use => self.parse_use().map(Item::Use),
            TokenKind::Impl => self.parse_impl().map(Item::Impl),
            TokenKind::Type => self.parse_alias().map(Item::Alias),
            TokenKind::Builtin | TokenKind::Ident if self.at_const(0) => {
                self.parse_const().map(Item::Const)
            }
            TokenKind::Builtin if self.at_check(0) => self.parse_check().map(Item::Const),
            _ => unreachable!("caller checks at_item"),
        };
        match item {
            Some(item) => self.add_item(item),
            None => self.recover(RecoveryLevel::Declaration),
        }
    }

    /// Who can see an `impl` block or an `extern` block is said on what's in it. A `use` can
    /// be `pub`, passing on what it brings; not `mod`.
    fn check_container_visibility(&mut self, at: som_ast::Span) {
        if self.visibility == Visibility::Private
            || self.visibility == Visibility::Public && self.peek() == TokenKind::Use
            || !matches!(
                self.peek(),
                TokenKind::Use | TokenKind::Impl | TokenKind::Extern
            )
            || self.peek_nth(1) == TokenKind::Struct
        {
            return;
        }
        self.error_at(
            at,
            msg![
                "a ",
                code(self.peek().to_string()),
                " can't be ",
                code("pub"),
                " or ",
                code("mod")
            ],
            "who can see something is said on each item inside it",
        );
        self.visibility = Visibility::Private;
    }

    fn parse_implicit_main(&mut self) {
        let start = self.current_span();
        let mut stmts = Vec::new();
        let mut value = None;
        while !self.at_eof() {
            // Items may be interleaved with the statements.
            if self.at_item_for(false) {
                self.expand_for();
                continue;
            }
            if self.at_item() {
                self.parse_item();
                continue;
            }
            let before = self.pos;
            match self.parse_stmt_or_expr() {
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) if self.at_eof() => value = Some(expr),
                StmtOrExpr::Expr(expr) => {
                    self.error_missing("expected ;", "a semicolon after the expression");
                    if !self.keep_unterminated(expr, before, &mut stmts) {
                        break;
                    }
                }
                StmtOrExpr::Error => self.recover(RecoveryLevel::Statement),
            }
        }
        let span = self.span_from(&start);
        let body = self
            .ast
            .exprs
            .alloc(Expr::Block { stmts, value }, span.clone());
        let name = Ident {
            name: som_lang::ENTRY_POINT.into(),
            span: start,
        };
        let func = Func {
            body: Some(body),
            ..signature(None, Visibility::Private, name)
        };
        let id = self.ast.funcs.alloc(func, span);
        self.add_item(Item::Func(id));
    }

    /// `type Vector3 = Vector<3, f32>;`, or `type Vector3<T> = Vector<3, T>;`.
    fn parse_alias(&mut self) -> Option<Alias> {
        let doc = self.doc_comment();
        self.expect(TokenKind::Type, "a type alias")?;
        let name = self.parse_ident("a type alias name")?;
        let generics = self.parse_generics(false)?;
        self.expect(TokenKind::Equals, "`=` and the type")?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::Semicolon, "a semicolon after the type alias")?;
        Some(Alias {
            doc,
            visibility: std::mem::take(&mut self.visibility),
            name,
            generics,
            ty,
        })
    }

    /// At `@const NAME:` or `@const NAME =`, `at` tokens ahead. Another language's
    /// `const NAME =` too, reported when it's read.
    pub(crate) fn at_const(&self, at: usize) -> bool {
        let written = self.builtin_ahead(at) == Some(Builtin::Const);
        (written || self.foreign_keyword(at, &foreign::CONST).is_some())
            && self.peek_nth(at + 1) == TokenKind::Ident
            && matches!(self.peek_nth(at + 2), TokenKind::Colon | TokenKind::Equals)
    }

    /// At a call made while compiling, on its own, `at` tokens ahead: `@assert(…);`. Or at
    /// `@const` without a name after it, which is reported.
    fn at_check(&self, at: usize) -> bool {
        let unnamed = self.builtin_ahead(at) == Some(Builtin::Const) && !self.at_const(at);
        unnamed || self.at_comptime_call(at)
    }

    /// `@assert(SIZE % 8 == 0, "…");`: code run while compiling for what it checks.
    fn parse_check(&mut self) -> Option<Id<Const>> {
        let start = self.current_span();
        if !self.at_comptime_call(0) {
            self.error_at(
                start.clone(),
                msg!["a constant needs a name: ", code("@const NAME = …;")],
                msg![
                    "to run a check while compiling, call it with ",
                    code("@"),
                    ": ",
                    code("@assert(…);")
                ],
            );
            self.advance();
        }
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semicolon, "a semicolon after it")?;
        let span = self.span_from(&start);
        let name = Ident {
            name: Const::UNNAMED.into(),
            span: start,
        };
        let c = Const {
            doc: None,
            visibility: Visibility::Private,
            name,
            ty: None,
            value,
            in_place: false,
        };
        Some(self.ast.consts.alloc(c, span))
    }

    /// `@const W: i32 = 60;`
    pub(crate) fn parse_const(&mut self) -> Option<Id<Const>> {
        let doc = self.doc_comment();
        let start = self.current_span();
        // `const MAX = 10;`: another language's, reported and read as `@const`.
        if let Some(found) = self.foreign_keyword(0, &foreign::CONST) {
            let message = msg![
                "som declares constants with ",
                code(found.som),
                ", not ",
                code(found.written)
            ];
            let fix = Fix::change_to(start.clone(), found.som);
            self.report_foreign(start.clone(), message, found, Some(fix));
        }
        self.advance();
        let name = self.parse_ident("a constant name")?;
        let ty = match self.at(TokenKind::Colon) {
            true => {
                self.advance();
                Some(self.parse_type()?)
            }
            false => None,
        };
        self.expect(TokenKind::Equals, "`=` and the constant's value")?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semicolon, "a semicolon after the constant")?;
        let span = self.span_from(&start);
        let c = Const {
            doc,
            visibility: std::mem::take(&mut self.visibility),
            name,
            ty,
            value,
            in_place: false,
        };
        Some(self.ast.consts.alloc(c, span))
    }

    pub(crate) fn parse_use(&mut self) -> Option<Use> {
        let public = std::mem::take(&mut self.visibility) == Visibility::Public;
        self.expect(TokenKind::Use, "a use statement")?;
        let start = self.current_span();
        let mut path = self.parse_path("an import path")?;
        // `use std.io`: other languages' paths, reported and read as the `std::io` meant.
        if self.at(TokenKind::Dot) && self.peek_nth(1) == TokenKind::Ident {
            while self.at(TokenKind::Dot) && self.peek_nth(1) == TokenKind::Ident {
                self.advance();
                path.segments.push(self.parse_ident("a module name")?);
            }
            let span = self.span_from(&start);
            let written: Vec<&str> = path.segments.iter().map(|s| &*s.name).collect();
            let fixed = written.join("::");
            self.error(
                span.clone(),
                msg![
                    "som separates a path with ",
                    code("::"),
                    ", not ",
                    code(".")
                ],
                msg!["write it ", code(&fixed)],
                Some(Fix::change_to(span, fixed)),
            );
        }
        self.expect(TokenKind::Semicolon, "a semicolon after the use statement")?;
        Some(Use { path, public })
    }
}

/// A function named `name` with nothing else said of it yet.
fn signature(doc: Option<String>, visibility: Visibility, name: Ident) -> Func {
    Func {
        doc,
        visibility,
        name,
        generics: Vec::new(),
        params: Vec::new(),
        returns: None,
        body: None,
        variadic: false,
        link_name: None,
        library: None,
        in_trait: false,
        on_values: false,
        prop: false,
        requires: Vec::new(),
        listens: Vec::new(),
        test: false,
        when: Vec::new(),
    }
}
