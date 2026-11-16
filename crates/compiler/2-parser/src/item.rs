use som_ast::{
    Alias, Const, AssocType, Expr, Extern, Field, Func, Id, Ident, Impl, Item, Param, Struct, Trait, Use,
    Variant, VariantKind, Visibility,
};
use som_base::message::code;
use som_base::{Fix, Span, foreign, msg};
use som_lexer::TokenKind;

use crate::{ParseError, Parser, RecoveryLevel, StmtOrExpr};

impl Parser<'_> {
    /// At `@where`, a condition on the function below.
    pub(crate) fn at_where(&self) -> bool {
        self.at(TokenKind::Builtin) && &*self.peek_token().text == "@where"
    }

    /// `@where i < self.len`, each line of them: kept for the function after them, with the
    /// `///` comment above them.
    fn parse_wheres(&mut self) {
        if self.wheres.is_empty() {
            self.where_doc = self.doc_comment();
            self.where_start = Some(self.current_span());
        }
        while self.at_where() {
            self.advance();
            match self.parse_expr() {
                Some(condition) => {
                    // `, "a position in it"`: what it means, for error messages.
                    let reason = match self.eat(TokenKind::Comma) && self.at(TokenKind::Text) {
                        true => {
                            let text = self.peek_token().text.clone();
                            self.advance();
                            Some(text[1..text.len() - 1].replace("\\\"", "\"").into())
                        }
                        false => None,
                    };
                    self.wheres.push(som_ast::Where { condition, reason });
                }
                None => {
                    self.recover(RecoveryLevel::Member);
                    return;
                }
            }
        }
        let at_function = {
            let mut at = 0;
            while matches!(self.peek_nth(at), TokenKind::Pub | TokenKind::Mod) {
                at += 1;
            }
            matches!(self.peek_nth(at), TokenKind::Fn | TokenKind::Prop)
        };
        if !at_function {
            let span = self
                .where_start
                .take()
                .unwrap_or_else(|| self.current_span());
            self.wheres.clear();
            self.where_doc = None;
            self.error_at(
                span,
                msg![code("@where"), " is for a function"],
                msg!["put it on the line above a ", code("fn")],
            );
        }
    }

    /// The `///` comment and `@where` conditions of the function starting here.
    fn take_wheres(&mut self) -> (Option<String>, Vec<som_ast::Where>) {
        let requires = std::mem::take(&mut self.wheres);
        self.where_start = None;
        match self.where_doc.take() {
            Some(doc) => (Some(doc), requires),
            None if !requires.is_empty() => (None, requires),
            None => (self.doc_comment(), requires),
        }
    }

    pub(crate) fn at_item(&self) -> bool {
        // `pub fn …`: who can see it, then the item. Other languages' `public` and
        // `static` too, reported when the item's read.
        let mut at = 0;
        while matches!(self.peek_nth(at), TokenKind::Pub | TokenKind::Mod)
            || self.foreign_modifier(at).is_some()
        {
            at += 1;
        }
        self.at_where()
            || matches!(
                self.peek_nth(at),
                TokenKind::Fn
                    | TokenKind::Extern
                    | TokenKind::Struct
                    | TokenKind::Enum
                    | TokenKind::Trait
                    | TokenKind::Use
                    | TokenKind::Impl
                    | TokenKind::Type
            )
            || self.at_const(at)
            || self.foreign_fn(at)
            || self.c_style_fn(at)
            || self.foreign_struct(at)
    }

    fn parse_item(&mut self) {
        self.in_impl = false;
        if self.at_where() {
            self.parse_wheres();
            return;
        }
        let at = self.current_span();
        self.visibility = self.parse_visibility();
        // Who can see `use`, an `impl` block or an `extern` block is said on what's in it.
        if self.visibility != Visibility::Private
            && matches!(
                self.peek(),
                TokenKind::Use | TokenKind::Impl | TokenKind::Extern
            )
            && self.peek_nth(1) != TokenKind::Struct
        {
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
            TokenKind::Builtin | TokenKind::Ident if self.at_const(0) => self.parse_const().map(Item::Const),
            _ => unreachable!("caller checks at_item"),
        };
        match item {
            Some(item) => self.add_item(item),
            None => self.recover(RecoveryLevel::Declaration),
        }
    }

    /// A file is a list of items, optionally followed by loose statements that
    /// form the body of an implicit `fn main()`.
    pub(crate) fn parse_program(&mut self) {
        while !self.at_eof() && self.at_item() {
            self.parse_item();
        }
        if !self.at_eof() {
            self.parse_implicit_main();
        }
    }

    fn parse_implicit_main(&mut self) {
        let start = self.current_span();
        let mut stmts = Vec::new();
        let mut value = None;

        while !self.at_eof() {
            // Items may be interleaved with the statements.
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
        let name = som_ast::Ident {
            name: "main".into(),
            span: start.clone(),
        };
        let func = Func {
            doc: None,
            visibility: Visibility::Private,
            name,
            generics: Vec::new(),
            params: Vec::new(),
            returns: None,
            body: Some(body),
            variadic: false,
            link_name: None,
            library: None,
            in_trait: false,
            prop: false,
            requires: Vec::new(),
        };
        let id = self.ast.funcs.alloc(func, span);
        self.add_item(Item::Func(id));
    }

    /// `impl<T> Pair<T> { … }`, or `impl<T> Iterator for ListIter<T> { type Item = &T; … }`.
    fn parse_impl(&mut self) -> Option<Impl> {
        self.expect(TokenKind::Impl, "an impl block")?;
        let generics = self.parse_generics(false)?;
        // `impl i32 { … }`: methods for a built-in type.
        if !self.at(TokenKind::Ident) {
            let ty = self.parse_type()?;
            return self.parse_impl_body(generics, None, None, Vec::new(), Some(ty));
        }
        let first = self.parse_path("a struct or trait name")?;
        let first_args = self.parse_type_args()?;
        // `Iterator for ListIter<T>`: what came first was the trait.
        let (trait_, target, target_args, target_ty) = if self.eat(TokenKind::For) {
            // `T?` is a type, not a struct's name.
            if self.at(TokenKind::Ident) && self.peek_nth(1) != TokenKind::Question {
                let target = self.parse_path("a struct name")?;
                let target_args = self.parse_type_args()?;
                (Some((first, first_args)), target, target_args, None)
            } else {
                // A type that isn't a struct: `bool`, `[T; N]`, `&T`, a slice's `[T]`.
                self.in_impl_target = true;
                let ty = self.parse_type();
                self.in_impl_target = false;
                let ty = ty?;
                let span = self.ast.types.span(ty).clone();
                let target = som_ast::Path {
                    segments: vec![som_ast::Ident {
                        name: span.get_text().as_ref().into(),
                        span,
                    }],
                };
                (Some((first, first_args)), target, Vec::new(), Some(ty))
            }
        } else {
            (None, first, first_args, None)
        };
        self.parse_impl_body(generics, trait_, Some(target), target_args, target_ty)
    }

    /// An impl block's `{ … }`, after its header. `target` is `None` for a type that isn't a
    /// struct, named by `target_ty` alone.
    fn parse_impl_body(
        &mut self,
        generics: Vec<som_ast::Generic>,
        trait_: Option<(som_ast::Path, Vec<Id<som_ast::TypeExpr>>)>,
        target: Option<som_ast::Path>,
        target_args: Vec<Id<som_ast::TypeExpr>>,
        target_ty: Option<Id<som_ast::TypeExpr>>,
    ) -> Option<Impl> {
        // A built-in type's is named by its text, for showing.
        let target = target.unwrap_or_else(|| {
            let span = self
                .ast
                .types
                .span(target_ty.expect("a type if not a path"))
                .clone();
            som_ast::Path {
                segments: vec![som_ast::Ident {
                    name: span.get_text().as_ref().into(),
                    span,
                }],
            }
        });
        self.expect(TokenKind::OpenBrace, "an impl block")?;

        let mut methods = Vec::new();
        let mut types = Vec::new();
        let mut consts = Vec::new();
        self.in_impl = true;
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at_where() {
                self.parse_wheres();
                continue;
            }
            // `pub fn …`, `mod prop …`: who can see the method.
            self.visibility = self.parse_visibility();
            // `type Item = &T;`
            if self.eat(TokenKind::Type) {
                let Some(name) = self.parse_ident("an associated type name") else {
                    self.recover(RecoveryLevel::Member);
                    continue;
                };
                self.expect(TokenKind::Equals, "`=` and the type")?;
                let ty = self.parse_type()?;
                self.expect(TokenKind::Semicolon, "a semicolon after the type")?;
                types.push((name, ty));
            } else if self.at_const(0) {
                match self.parse_const() {
                    Some(id) => {
                        // Its type and value can't depend on the block's parameters: it's one
                        // value, not one per type the block is for.
                        if let Some(first) = generics.first() {
                            let name = &self.ast.consts[id].name;
                            self.errors.push(ParseError {
                                message: "a constant can't be in an impl block with type parameters"
                                    .into(),
                                hint: msg![
                                    "it would be one value for every ",
                                    code(first.name.name.to_string()),
                                    "; put it outside the block, as a module's constant"
                                ],
                                fix: None,
                                span: name.span.clone(),
                            });
                        }
                        consts.push(id);
                    }
                    None => self.recover(RecoveryLevel::Member),
                }
            } else if self.at_fn() || self.at(TokenKind::Prop) || self.c_style_fn(0) {
                match self.parse_function() {
                    Some(id) => methods.push(id),
                    None => self.recover(RecoveryLevel::Member),
                }
            } else {
                self.error_missing("expected method declaration", "a method declaration");
                self.recover(RecoveryLevel::Member);
            }
        }

        self.in_impl = false;
        self.expect(TokenKind::CloseBrace, "a closing brace after impl block")?;
        let close = self.previous_span();
        // Every method is generic over the impl's parameters, ahead of its own.
        for &method in &methods {
            let own = std::mem::take(&mut self.ast.funcs.get_mut(method).generics);
            self.ast.funcs.get_mut(method).generics = generics.iter().cloned().chain(own).collect();
        }
        Some(Impl {
            target,
            target_args,
            target_ty,
            trait_,
            types,
            generics,
            methods,
            consts,
            close,
        })
    }

    /// `trait Iterator { type Item; fn next(self: &mut Self) -> Item?; }`. A method without a
    /// body ends in `;`.
    fn parse_trait(&mut self) -> Option<Id<Trait>> {
        let doc = self.doc_comment();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        self.expect(TokenKind::Trait, "a trait declaration")?;
        let name = self.parse_ident("a trait name")?;
        let generics = self.parse_generics(true)?;
        // `trait IndexMut: Index`
        let mut supers = Vec::new();
        if self.eat(TokenKind::Colon) {
            loop {
                supers.push(self.parse_path("a trait name")?);
                if !self.eat(TokenKind::Plus) {
                    break;
                }
            }
        }
        self.expect(TokenKind::OpenBrace, "the trait's methods")?;
        self.trait_supers = supers.clone();
        let mut types = Vec::new();
        let mut methods = Vec::new();
        self.in_impl = true;
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at(TokenKind::Type) {
                let doc = self.doc_comment();
                self.advance();
                let Some(name) = self.parse_ident("an associated type name") else {
                    self.recover(RecoveryLevel::Member);
                    continue;
                };
                self.expect(TokenKind::Semicolon, "a semicolon after the type")?;
                types.push(AssocType { doc, name });
            } else if self.at_where() {
                self.parse_wheres();
            } else if self.at_fn() || self.at(TokenKind::Prop) {
                match self.parse_trait_method(&name, &generics) {
                    Some(id) => methods.push(id),
                    None => self.recover(RecoveryLevel::Member),
                }
            } else {
                self.error_missing(
                    "expected a method or `type` declaration",
                    "a method or associated type",
                );
                self.recover(RecoveryLevel::Member);
            }
        }
        self.in_impl = false;
        self.expect(TokenKind::CloseBrace, "a closing brace after the trait")?;
        let span = self.span_from(&start);
        Some(self.ast.traits.alloc(
            Trait {
                doc,
                visibility,
                name,
                generics,
                types,
                supers,
                methods,
            },
            span,
        ))
    }

    /// A trait's method: a signature ending in `;`, or with a body. It's generic over `Self`,
    /// the type implementing it, then the trait's generics, then its own.
    fn parse_trait_method(
        &mut self,
        trait_name: &Ident,
        trait_generics: &[som_ast::Generic],
    ) -> Option<Id<Func>> {
        let (doc, requires) = self.take_wheres();
        let start = self.current_span();
        let prop = self.eat(TokenKind::Prop);
        if !prop {
            self.expect_fn("a method declaration")?;
        }
        let name = self.parse_ident("a method name")?;
        let own = self.parse_generics(false)?;
        let (params, variadic) = match prop {
            true => (self.prop_params(&name)?, false),
            false => self.parse_params(false)?,
        };
        let returns = if self.void_return() {
            None
        } else if self.eat(TokenKind::Arrow) || prop && self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = if self.eat(TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_body("method")?)
        };
        let this = som_ast::Generic {
            name: Ident {
                name: "Self".into(),
                span: trait_name.span.clone(),
            },
            ty: None,
            default: None,
            bounds: std::iter::once(som_ast::Path {
                segments: vec![trait_name.clone()],
            })
            .chain(self.trait_supers.iter().cloned())
            .collect(),
        };
        // The defaults are the trait's: a method's copies don't have them.
        let generics = std::iter::once(this)
            .chain(trait_generics.iter().map(|g| som_ast::Generic {
                default: None,
                ..g.clone()
            }))
            .chain(own)
            .collect();
        let span = self.span_from(&start);
        let func = Func {
            doc,
            // As visible as the trait.
            visibility: Visibility::Public,
            name,
            generics,
            params,
            returns,
            body,
            variadic,
            link_name: None,
            library: None,
            in_trait: true,
            prop,
            requires,
        };
        Some(self.ast.funcs.alloc(func, span))
    }

    /// `use extern "raylib" …`: an extern block whose library module is also `use`d.
    fn parse_use_extern(&mut self) -> Option<Extern> {
        self.expect(TokenKind::Use, "a use statement")?;
        let mut block = self.parse_extern()?;
        if block.library.is_none() {
            let span = self.previous_span();
            self.error_at(
                span,
                msg!["only a library's declarations can be ", code("use"), "d"],
                msg!["name the library: ", code("use extern \"lib\" { … }")],
            );
        }
        block.use_module = true;
        Some(block)
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
        let written = (self.tokens[self.pos..].iter())
            .filter(|t| !crate::is_trivia(t.kind))
            .nth(at)
            .is_some_and(|t| &*t.text == "@const");
        (written || self.foreign_keyword(at, foreign::const_word).is_some())
            && self.peek_nth(at + 1) == TokenKind::Ident
            && matches!(
                self.peek_nth(at + 2),
                TokenKind::Colon | TokenKind::Equals
            )
    }

    /// `@const W: i32 = 60;`
    fn parse_const(&mut self) -> Option<Id<Const>> {
        let doc = self.doc_comment();
        let start = self.current_span();
        // `const MAX = 10;`: another language's, reported and read as `@const`.
        if let Some(found) = self.foreign_keyword(0, foreign::const_word) {
            let message = msg![
                "som declares constants with ",
                code(found.som),
                ", not ",
                code(found.written)
            ];
            let fix = Fix::replace(msg!["change to ", code(found.som)], start.clone(), found.som);
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
        Some(self.ast.consts.alloc(
            Const {
                doc,
                visibility: std::mem::take(&mut self.visibility),
                name,
                ty,
                value,
            },
            span,
        ))
    }

    pub(crate) fn parse_use(&mut self) -> Option<Use> {
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
            self.errors.push(ParseError {
                message: msg![
                    "som separates a path with ",
                    code("::"),
                    ", not ",
                    code(".")
                ],
                hint: msg!["write it ", code(&fixed)],
                fix: Some(Fix::replace(
                    msg!["change to ", code(&fixed)],
                    span.clone(),
                    fixed,
                )),
                span,
            });
        }
        self.expect(TokenKind::Semicolon, "a semicolon after the use statement")?;
        Some(Use { path })
    }

    /// `struct Point { x: i32, y: i32 }`, or `extern struct Sqlite3;`: a struct only C
    /// sees inside, used through handles to one.
    fn parse_struct(&mut self) -> Option<Id<Struct>> {
        let doc = self.doc_comment();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        let handle = self.eat(TokenKind::Extern);
        self.expect_struct("a struct declaration")?;
        let name = self.parse_ident("a struct name")?;
        let generics = self.parse_generics(true)?;

        let fields = if handle {
            self.expect(TokenKind::Semicolon, "a semicolon after an extern struct")?;
            Vec::new()
        } else {
            self.expect(TokenKind::OpenBrace, "a list of struct fields")?;
            // Methods written among the fields, as in another language's class: skipped,
            // with where they are, to move to an `impl` block. Whether one's a constructor.
            let mut methods: Vec<(Span, Span, Span)> = Vec::new();
            let mut constructor = false;
            let mut fields = Vec::new();
            while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
                if let Some((is_constructor, name_at)) = self.member_method(&name.name) {
                    let start = self.current_span();
                    let header = start.merge(&self.span_ahead(name_at));
                    let from = self.previous_span().after();
                    self.skip_member();
                    constructor |= is_constructor;
                    methods.push((self.span_from(&start), from, header));
                    continue;
                }
                let before = self.pos;
                let (field, separated) = self.parse_field()?;
                fields.push(field);
                if self.pos == before {
                    return None;
                }
                if !separated && !self.list_continues(TokenKind::CloseBrace) {
                    break;
                }
            }
            let close = self.current_span();
            self.expect(TokenKind::CloseBrace, "a closing brace after struct fields")?;
            if !methods.is_empty() {
                self.methods_in_struct(&name.name, &methods, constructor, &close);
            }
            fields
        };

        let span = self.span_from(&start);
        Some(self.ast.structs.alloc(
            Struct {
                doc,
                visibility,
                name,
                generics,
                fields,
                variants: None,
                // A handle is just a pointer, so copying it copies nothing C owns.
                copy: handle,
                handle,
            },
            span,
        ))
    }

    /// A struct field, `name: Type`, or another language's `Type name;`, reported and read as
    /// the `name: Type,` it means. Whether that took its separator, the `;`, along.
    fn parse_field(&mut self) -> Option<(Field, bool)> {
        let doc = self.doc_comment();
        let visibility = self.parse_visibility();
        let type_first = (self.at(TokenKind::Ident) || crate::ty::primitive(self.peek()).is_some())
            && self.peek_nth(1) == TokenKind::Ident
            && matches!(
                self.peek_nth(2),
                TokenKind::Semicolon | TokenKind::Comma | TokenKind::CloseBrace
            );
        if !type_first {
            let name = self.parse_ident("a struct field name")?;
            self.expect(TokenKind::Colon, "a struct field type")?;
            let ty = self.parse_type()?;
            return Some((
                Field {
                    doc,
                    visibility,
                    name,
                    ty,
                },
                false,
            ));
        }
        let start = self.current_span();
        let ty = self.parse_type()?;
        let ty_text = self.span_from(&start).get_text();
        let name = self.parse_ident("a struct field name")?;
        let separated = self.eat(TokenKind::Semicolon) || self.eat(TokenKind::Comma);
        let span = self.span_from(&start);
        let fixed = format!(
            "{}: {ty_text}{}",
            name.name,
            if separated { "," } else { "" }
        );
        if !self.in_recovery {
            self.errors.push(ParseError {
                message: msg!["som writes a field's name first, then its type"],
                hint: msg!["write it ", code(&fixed)],
                fix: Some(Fix::replace(
                    msg!["change to ", code(&fixed)],
                    span.clone(),
                    fixed,
                )),
                span,
            });
        }
        Some((
            Field {
                doc,
                visibility,
                name,
                ty,
            },
            separated,
        ))
    }

    /// At a method among a struct's fields, as in another language's class: `fn area(…)`,
    /// `int area(…)`, or a constructor, `Point(…)`, after any modifiers. Whether it's a
    /// constructor, and how many tokens ahead its name is.
    fn member_method(&self, struct_name: &str) -> Option<(bool, usize)> {
        let mut at = 0;
        while matches!(self.peek_nth(at), TokenKind::Pub | TokenKind::Mod)
            || self.foreign_modifier(at).is_some()
        {
            at += 1;
        }
        if self.ident_ahead(at) == Some(struct_name)
            && self.peek_nth(at + 1) == TokenKind::OpenParen
        {
            return Some((true, at));
        }
        (self.peek_nth(at) == TokenKind::Fn || self.foreign_fn(at) || self.c_style_fn(at))
            .then_some((false, at + 1))
    }

    /// Skips a member up to the end of its body, `{ … }`, or its `;`.
    fn skip_member(&mut self) {
        let mut depth = 0usize;
        while !self.at_eof() {
            match self.peek() {
                TokenKind::OpenParen | TokenKind::OpenBracket => depth += 1,
                TokenKind::CloseParen | TokenKind::CloseBracket => depth = depth.saturating_sub(1),
                TokenKind::Semicolon if depth == 0 => {
                    self.advance();
                    return;
                }
                TokenKind::OpenBrace if depth == 0 => {
                    let mut braces = 0usize;
                    while !self.at_eof() {
                        match self.peek() {
                            TokenKind::OpenBrace => braces += 1,
                            TokenKind::CloseBrace => {
                                braces -= 1;
                                if braces == 0 {
                                    self.advance();
                                    return;
                                }
                            }
                            _ => {}
                        }
                        self.advance();
                    }
                    return;
                }
                // The struct's own `}`: a body never came.
                TokenKind::CloseBrace if depth == 0 => return,
                _ => {}
            }
            self.advance();
        }
    }

    /// Reports `methods` written among struct `name`'s fields: each its text, where it starts
    /// with the space before it, and its header, up to its name. The fix moves them, as they are, to an `impl` block
    /// after the struct's closing brace, `close`.
    fn methods_in_struct(
        &mut self,
        name: &str,
        methods: &[(Span, Span, Span)],
        constructor: bool,
        close: &Span,
    ) {
        if self.in_recovery {
            return;
        }
        let mut edits = Vec::new();
        let mut moved = Vec::new();
        for (method, from, _) in methods {
            let removed = Span {
                end: method.end,
                length: method.start_offset + method.length - from.start_offset,
                ..from.clone()
            };
            edits.push((removed, String::new()));
            moved.push(format!("    {}", method.get_text()));
        }
        let block = format!("\n\nimpl {name} {{\n{}\n}}", moved.join("\n\n"));
        edits.push((close.after(), block));
        let header = methods[0].2.clone();
        let hint = if constructor {
            msg![
                "a constructor is a function returning the struct: ",
                code(format!("fn new(…) -> {name} {{ {name} {{ … }} }}"))
            ]
        } else {
            msg![
                "a method takes ",
                code("self"),
                ", ",
                code("&self"),
                " or ",
                code("&mut self")
            ]
        };
        self.errors.push(ParseError {
            message: msg![
                "a struct holds only its fields: methods go in ",
                code(format!("impl {name} {{ … }}"))
            ],
            hint,
            fix: Some(Fix {
                title: msg!["move them to ", code(format!("impl {name}"))],
                edits,
            }),
            span: header,
        });
    }

    /// `enum Shape { Circle(f32), Rect { w: f32, h: f32 }, Empty }`: a type whose values are
    /// each one of its variants.
    fn parse_enum(&mut self) -> Option<Id<Struct>> {
        let doc = self.doc_comment();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        self.expect(TokenKind::Enum, "an enum declaration")?;
        let name = self.parse_ident("an enum name")?;
        let generics = self.parse_generics(true)?;
        self.expect(TokenKind::OpenBrace, "a list of enum variants")?;
        let variants = self.parse_comma_list(TokenKind::CloseBrace, |p| p.parse_variant())?;
        self.expect(TokenKind::CloseBrace, "a closing brace after the variants")?;
        // Variants that hold nothing are just a number.
        let copy = variants.iter().all(|v| v.fields.is_empty());
        let span = self.span_from(&start);
        Some(self.ast.structs.alloc(
            Struct {
                doc,
                visibility,
                name,
                generics,
                fields: Vec::new(),
                variants: Some(variants),
                copy,
                handle: false,
            },
            span,
        ))
    }

    /// `Empty`, `Circle(f32)` or `Rect { w: f32, h: f32 }`
    fn parse_variant(&mut self) -> Option<Variant> {
        let doc = self.doc_comment();
        let name = self.parse_ident("a variant name")?;
        let (kind, fields) = if self.eat(TokenKind::OpenParen) {
            let types = self.parse_comma_list(TokenKind::CloseParen, |p| p.parse_type())?;
            self.expect(
                TokenKind::CloseParen,
                "a closing parenthesis after the fields",
            )?;
            let fields = (types.into_iter().enumerate())
                .map(|(i, ty)| Field {
                    doc: None,
                    visibility: Visibility::Public,
                    name: Ident {
                        name: i.to_string().into(),
                        span: self.ast.types.span(ty).clone(),
                    },
                    ty,
                })
                .collect();
            (VariantKind::Tuple, fields)
        } else if self.eat(TokenKind::OpenBrace) {
            let fields = self.parse_comma_list(TokenKind::CloseBrace, |p| {
                let doc = p.doc_comment();
                let name = p.parse_ident("a field name")?;
                p.expect(TokenKind::Colon, "a field type")?;
                let ty = p.parse_type()?;
                // As visible as the enum.
                Some(Field {
                    doc,
                    visibility: Visibility::Public,
                    name,
                    ty,
                })
            })?;
            self.expect(TokenKind::CloseBrace, "a closing brace after the fields")?;
            (VariantKind::Named, fields)
        } else {
            (VariantKind::Unit, Vec::new())
        };
        // `W = 87`, `Unknown = -1`: the number it converts to.
        let value = if self.at(TokenKind::Equals) {
            let at = self.current_span();
            self.advance();
            let negative = self.eat(TokenKind::Minus);
            let n = self.parse_count("the variant's number")?;
            if kind != VariantKind::Unit {
                self.error_at(
                    at.clone(),
                    "a variant holding values has no number",
                    "only variants that hold nothing convert to a number",
                );
            }
            match i128::try_from(n) {
                Ok(n) if negative => Some(-n),
                Ok(n) => Some(n),
                Err(_) => {
                    self.error_at(at, "number is too large", "this does not fit in an `i128`");
                    None
                }
            }
        } else {
            None
        };
        Some(Variant {
            doc,
            name,
            kind,
            value,
            fields,
        })
    }

    /// `fn name<T>(params) -> Ret { body }` or `fn name<T>(params) -> Ret = value;`, or in an
    /// impl block, `prop name: Ret { body }` or `prop name: Ret = value;`.
    fn parse_function(&mut self) -> Option<Id<Func>> {
        let (doc, requires) = self.take_wheres();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        let prop = self.eat(TokenKind::Prop);
        // `int sum()`: another language's function, its return type first.
        let c_return = if !prop && self.c_style_fn(0) {
            let ty_start = self.current_span();
            let ty = self.parse_type()?;
            Some((ty, self.span_from(&ty_start)))
        } else {
            None
        };
        if !prop && c_return.is_none() {
            self.expect_fn("a function declaration")?;
        }
        let name = self.parse_ident("a function name")?;

        let generics = self.parse_generics(false)?;
        let (params, variadic) = match prop {
            true => (self.prop_params(&name)?, false),
            false => self.parse_params(false)?,
        };
        // A property's type goes after a `:`, as a field's does: `prop x: i32 = …`.
        let returns = if let Some((ty, ty_span)) = c_return {
            // Reported, and read as the `fn name(…) -> Type` it means.
            let ty_text = ty_span.get_text();
            let before_name = Span {
                end: name.span.start,
                length: name.span.start_offset - ty_span.start_offset,
                ..ty_span.clone()
            };
            let returns = format!(" -> {ty_text}");
            if !self.in_recovery {
                self.errors.push(ParseError {
                    message: msg![
                        "som declares functions with ",
                        code("fn"),
                        ", with the return type after ",
                        code("->")
                    ],
                    hint: msg![
                        "write it ",
                        code(format!("fn {}(…) -> {ty_text}", name.name))
                    ],
                    fix: Some(Fix {
                        title: msg!["change to ", code(format!("fn {}(…){returns}", name.name))],
                        edits: vec![
                            (before_name, "fn ".to_string()),
                            (self.previous_span().after(), returns),
                        ],
                    }),
                    span: ty_span,
                });
            }
            Some(ty)
        } else if prop && self.at(TokenKind::Arrow) {
            let span = self.current_span();
            self.advance();
            self.error_at(
                span,
                msg!["a property's type goes after a ", code(":")],
                msg![
                    "as a field's does: ",
                    code(format!("prop {}: …", name.name))
                ],
            );
            Some(self.parse_type()?)
        } else if self.void_return() {
            None
        } else if self.eat(TokenKind::Arrow) || prop && self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = Some(self.parse_body(if prop { "property" } else { "function" })?);

        let span = self.span_from(&start);
        let func = Func {
            doc,
            visibility,
            name,
            generics,
            params,
            returns,
            body,
            variadic,
            link_name: None,
            library: None,
            in_trait: false,
            prop,
            requires,
        };
        Some(self.ast.funcs.alloc(func, span))
    }

    /// A function's or property's body: a block, or `= value;`, any expression, as in
    /// `fn double(x: i32) = x * 2;` and `prop x = self.values[0];`. `what` it's the body of,
    /// for errors.
    fn parse_body(&mut self, what: &str) -> Option<Id<Expr>> {
        if !self.eat(TokenKind::Equals) {
            return self.parse_block();
        }
        let value = self.parse_expr()?;
        self.expect(
            TokenKind::Semicolon,
            format!("a semicolon after the {what}'s value"),
        )?;
        Some(value)
    }

    /// A property's parameters, which aren't written: just `&self`. Ones written anyway are
    /// reported and skipped.
    fn prop_params(&mut self, name: &Ident) -> Option<Vec<Param>> {
        if self.at(TokenKind::OpenParen) {
            let start = self.current_span();
            self.parse_params(false)?;
            let span = self.span_from(&start);
            self.error_at(
                span,
                "a property has no parameters",
                msg![
                    "its ",
                    code("self"),
                    " is always borrowed: ",
                    code(format!("prop {} -> …", name.name))
                ],
            );
        }
        let ty = self.self_type(Some(false), name.span.clone());
        Some(vec![Param {
            name: Ident {
                name: "self".into(),
                span: name.span.clone(),
            },
            ty: Some(ty),
            out: false,
            mutable: false,
        }])
    }

    /// `Self`, or with `reference` saying whether it's mutable, `&Self` or `&mut Self`: the
    /// type of a `self`, `&self` or `&mut self` parameter, written at `span`.
    fn self_type(
        &mut self,
        reference: Option<bool>,
        span: som_base::Span,
    ) -> Id<som_ast::TypeExpr> {
        let path = som_ast::Path {
            segments: vec![Ident {
                name: "Self".into(),
                span: span.clone(),
            }],
        };
        let named = som_ast::TypeExpr::Named {
            path,
            args: Vec::new(),
        };
        let to = self.ast.types.alloc(named, span.clone());
        match reference {
            Some(mutable) => self.ast.types.alloc(
                som_ast::TypeExpr::Reference {
                    mutable,
                    lifetime: None,
                    to,
                },
                span,
            ),
            None => to,
        }
    }

    /// `<T, N: usize>` declaring generic parameters, if it's there: a type parameter is a
    /// name, a value parameter a name and its type.
    /// `<T, N: usize, Other = Self>`. Only a struct's or a trait's parameters can have a
    /// default, `with_defaults`.
    fn parse_generics(&mut self, with_defaults: bool) -> Option<Vec<som_ast::Generic>> {
        if !self.eat(TokenKind::LessThan) {
            return Some(Vec::new());
        }
        let mut defaulted = false;
        let params = self.parse_comma_list(TokenKind::GreaterThan, |p| {
            let name = p.parse_ident("a generic parameter name")?;
            let (mut ty, mut bounds) = (None, Vec::new());
            if p.eat(TokenKind::Colon) {
                // `N: usize` is a value parameter; `T: Hash + Eq` a type that has to implement
                // traits.
                let primitive = p.at_primitive_type();
                if primitive {
                    ty = Some(p.parse_type()?);
                } else {
                    bounds.push(p.parse_path("a trait")?);
                    while p.eat(TokenKind::Plus) {
                        bounds.push(p.parse_path("a trait")?);
                    }
                }
            }
            let mut default = None;
            let at = p.current_span();
            if p.eat(TokenKind::Equals) {
                default = Some(p.parse_type()?);
                if !with_defaults {
                    p.error_at(
                        at.clone(),
                        "only a struct's or a trait's parameters have defaults",
                        "a default isn't allowed here",
                    );
                } else if ty.is_some() {
                    p.error_at(
                        at.clone(),
                        "a value parameter has no default",
                        "a default isn't allowed here",
                    );
                }
                defaulted = true;
            } else if defaulted && with_defaults {
                // `<A = i32, B>`: which one does `<u8>` give?
                p.error_at(
                    name.span.clone(),
                    "a parameter without a default can't come after one with a default",
                    "give it a default, or move it before those that have one",
                );
            }
            Some(som_ast::Generic {
                name,
                ty,
                bounds,
                default,
            })
        })?;
        self.expect(
            TokenKind::GreaterThan,
            "a closing angle bracket for type parameters",
        )?;
        Some(params)
    }

    /// `(a: i32, b)`. Extern functions must annotate every parameter, and may end with
    /// `...` to take extra arguments, like C's `printf`; the `bool` says whether they do.
    fn parse_params(&mut self, is_extern: bool) -> Option<(Vec<Param>, bool)> {
        let outer = std::mem::replace(&mut self.in_params, true);
        let params = self.parse_params_in(is_extern);
        self.in_params = outer;
        params
    }

    fn parse_params_in(&mut self, is_extern: bool) -> Option<(Vec<Param>, bool)> {
        self.expect(TokenKind::OpenParen, "a list of function parameters")?;
        let mut params = Vec::new();
        let mut variadic = false;
        while !self.at(TokenKind::CloseParen) && !self.at_eof() {
            if self.at(TokenKind::Ellipsis) {
                let span = self.current_span();
                self.advance();
                if !is_extern {
                    self.error_at(
                        span,
                        "only extern functions can take extra arguments",
                        "`...` isn't allowed here",
                    );
                } else if params.is_empty() {
                    self.error_at(
                        span,
                        msg!["a ", code("..."), " needs a parameter before it"],
                        "C requires one",
                    );
                } else {
                    variadic = true;
                }
                if !self.at(TokenKind::CloseParen) {
                    self.error_missing(
                        msg![code("..."), " must come last"],
                        "a closing parenthesis",
                    );
                    return None;
                }
                break;
            }
            // `&self` and `&mut self`: short for `self: &Self` and `self: &mut Self`.
            let reference = (self.at(TokenKind::Ampersand)
                && (self.self_ahead(1)
                    || self.peek_nth(1) == TokenKind::Mut && self.self_ahead(2)))
            .then(|| {
                let start = self.current_span();
                self.advance();
                (self.eat(TokenKind::Mut), start)
            });
            // `string name`: another language's order, the type first. Reported, and read as
            // the `name: string` it means.
            if reference.is_none()
                && (self.at(TokenKind::Ident) || crate::ty::primitive(self.peek()).is_some())
                && self.peek_nth(1) == TokenKind::Ident
                && matches!(self.peek_nth(2), TokenKind::Comma | TokenKind::CloseParen)
            {
                let start = self.current_span();
                let ty = self.parse_type()?;
                let ty_text = self.span_from(&start).get_text();
                let name = self.parse_ident("a function parameter name")?;
                let span = self.span_from(&start);
                let fixed = format!("{}: {ty_text}", name.name);
                self.errors.push(ParseError {
                    message: msg!["som writes a parameter's name first, then its type"],
                    hint: msg!["write it ", code(&fixed)],
                    fix: Some(Fix::replace(
                        msg!["change to ", code(&fixed)],
                        span.clone(),
                        fixed,
                    )),
                    span,
                });
                params.push(Param {
                    name,
                    ty: Some(ty),
                    out: false,
                    mutable: false,
                });
                if !self.list_continues(TokenKind::CloseParen) {
                    break;
                }
                continue;
            }
            let out = reference.is_none() && self.eat(TokenKind::Out);
            let mutable = reference.is_none() && !out && self.eat(TokenKind::Mut);
            let name = match self.at(TokenKind::SelfValue) {
                true => self.parse_self(),
                false => self.parse_ident("a function parameter name")?,
            };
            let shorthand =
                reference.is_some() || &*name.name == "self" && !self.at(TokenKind::Colon);
            if shorthand && !self.in_impl {
                let span = match &reference {
                    Some((_, start)) => self.span_from(start),
                    None => name.span.clone(),
                };
                self.error_at(
                    span,
                    msg![code("self"), " only exists in methods"],
                    msg![
                        "a method is written in an ",
                        code("impl"),
                        " or ",
                        code("trait"),
                        " block"
                    ],
                );
            }
            let ty = if let Some((mutable, start)) = reference {
                Some(self.self_type(Some(mutable), self.span_from(&start)))
            } else if is_extern {
                self.expect(TokenKind::Colon, "a colon after function parameter name")?;
                Some(self.parse_type()?)
            } else if self.eat(TokenKind::Colon) {
                Some(self.parse_type()?)
            } else if &*name.name == "self" {
                // `self` and `mut self`: short for `self: Self`.
                Some(self.self_type(None, name.span.clone()))
            } else {
                None
            };
            params.push(Param {
                name,
                ty,
                out,
                mutable,
            });
            if !self.list_continues(TokenKind::CloseParen) {
                break;
            }
        }
        self.expect(
            TokenKind::CloseParen,
            "a closing parenthesis after function parameters",
        )?;
        Some((params, variadic))
    }

    /// `extern "lib" { fn foo(); }`
    fn parse_extern(&mut self) -> Option<Extern> {
        let start = self.current_span();
        self.expect(TokenKind::Extern, "an extern block")?;

        let (library, library_span) = if self.at(TokenKind::Text) {
            let token = self.peek_token();
            let library = token.text[1..token.text.len() - 1].to_string();
            let span = token.span.clone();
            self.advance();
            (Some(library), Some(span))
        } else {
            (None, None)
        };

        // `from "raylib.h"` imports the declarations from a C header. `from` is only a
        // keyword here, so it can still name things elsewhere.
        let header = if self.at(TokenKind::Ident) && &*self.peek_token().text == "from" {
            self.advance();
            if !self.at(TokenKind::Text) {
                self.error_expected(&[TokenKind::Text], "a header file name");
                return None;
            }
            let token = self.peek_token();
            let header = (
                token.text[1..token.text.len() - 1].to_string(),
                token.span.clone(),
            );
            self.advance();
            Some(header)
        } else {
            None
        };
        // `extern "raylib";` is short for `extern "raylib" from "raylib.h";`.
        if (header.is_some() || library.is_some()) && self.eat(TokenKind::Semicolon) {
            let header = header.or_else(|| {
                let span = library_span.clone().expect("a library has a span");
                Some((
                    format!("{}.h", library.as_ref().expect("checked above")),
                    span,
                ))
            });
            return Some(Extern {
                span: self.span_from(&start),
                library,
                library_span,
                header,
                use_module: false,
                module: None,
                funcs: Vec::new(),
            });
        }

        self.expect(
            TokenKind::OpenBrace,
            "a list of extern function declarations",
        )?;
        let mut funcs = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            self.visibility = self.parse_visibility();
            if self.at_fn() {
                match self.parse_extern_func() {
                    Some(id) => funcs.push(id),
                    None => self.recover(RecoveryLevel::Declaration),
                }
            } else {
                self.error_missing(
                    "expected extern function declaration",
                    "an extern function declaration",
                );
                self.recover(RecoveryLevel::Declaration);
            }
        }
        self.expect(TokenKind::CloseBrace, "a closing brace after extern block")?;

        Some(Extern {
            span: self.span_from(&start),
            library,
            library_span,
            header,
            use_module: false,
            module: None,
            funcs,
        })
    }

    /// `fn name(params) -> Ret;`
    fn parse_extern_func(&mut self) -> Option<Id<Func>> {
        let doc = self.doc_comment();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        self.expect_fn("a function declaration")?;
        let name = self.parse_ident("a function name")?;
        let (params, variadic) = self.parse_params(true)?;
        let returns = if self.void_return() {
            None
        } else if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(
            TokenKind::Semicolon,
            "a semicolon after extern function declaration",
        )?;

        let span = self.span_from(&start);
        let func = Func {
            doc,
            visibility,
            name,
            generics: Vec::new(),
            params,
            returns,
            body: None,
            variadic,
            link_name: None,
            library: None,
            in_trait: false,
            prop: false,
            requires: Vec::new(),
        };
        Some(self.ast.funcs.alloc(func, span))
    }
}
