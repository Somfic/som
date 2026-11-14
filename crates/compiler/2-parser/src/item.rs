use som_ast::{
    AssocType, Expr, Extern, Field, Func, Id, Ident, Impl, Item, Param, Struct, Trait, Use,
    Variant, VariantKind,
};
use som_base::message::code;
use som_base::msg;
use som_lexer::TokenKind;

use crate::{Parser, RecoveryLevel, StmtOrExpr};

impl Parser<'_> {
    fn at_item(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Fn
                | TokenKind::Extern
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Trait
                | TokenKind::Use
                | TokenKind::Impl
        )
    }

    fn parse_item(&mut self) {
        let item = match self.peek() {
            TokenKind::Fn => self.parse_function().map(Item::Func),
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
            name,
            generics: Vec::new(),
            params: Vec::new(),
            returns: None,
            body: Some(body),
            variadic: false,
            link_name: None,
            library: None,
            in_trait: false,
        };
        let id = self.ast.funcs.alloc(func, span);
        self.add_item(Item::Func(id));
    }

    /// `impl<T> Pair<T> { … }`, or `impl<T> Iterator for ListIter<T> { type Item = &T; … }`.
    fn parse_impl(&mut self) -> Option<Impl> {
        self.expect(TokenKind::Impl, "an impl block")?;
        let generics = self.parse_generics()?;
        let first = self.parse_path("a struct or trait name")?;
        let first_args = self.parse_type_args()?;
        // `Iterator for ListIter<T>`: what came first was the trait.
        let (trait_, target, target_args) = if self.eat(TokenKind::For) {
            let target = self.parse_path("a struct name")?;
            let target_args = self.parse_type_args()?;
            (Some((first, first_args)), target, target_args)
        } else {
            (None, first, first_args)
        };
        self.expect(TokenKind::OpenBrace, "an impl block")?;

        let mut methods = Vec::new();
        let mut types = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            // `type Item = &T;`
            if self.eat(TokenKind::Type) {
                let Some(name) = self.parse_ident("an associated type name") else {
                    self.recover(RecoveryLevel::Declaration);
                    continue;
                };
                self.expect(TokenKind::Equals, "`=` and the type")?;
                let ty = self.parse_type()?;
                self.expect(TokenKind::Semicolon, "a semicolon after the type")?;
                types.push((name, ty));
            } else if self.at(TokenKind::Fn) {
                match self.parse_function() {
                    Some(id) => methods.push(id),
                    None => self.recover(RecoveryLevel::Declaration),
                }
            } else {
                self.error_missing("expected method declaration", "a method declaration");
                self.recover(RecoveryLevel::Declaration);
            }
        }

        self.expect(TokenKind::CloseBrace, "a closing brace after impl block")?;
        // Every method is generic over the impl's parameters, ahead of its own.
        for &method in &methods {
            let own = std::mem::take(&mut self.ast.funcs.get_mut(method).generics);
            self.ast.funcs.get_mut(method).generics = generics.iter().cloned().chain(own).collect();
        }
        Some(Impl {
            target,
            target_args,
            trait_,
            types,
            generics,
            methods,
        })
    }

    /// `trait Iterator { type Item; fn next(self: &mut Self) -> Item?; }`. A method without a
    /// body ends in `;`.
    fn parse_trait(&mut self) -> Option<Id<Trait>> {
        let doc = self.doc_comment();
        let start = self.current_span();
        self.expect(TokenKind::Trait, "a trait declaration")?;
        let name = self.parse_ident("a trait name")?;
        let generics = self.parse_generics()?;
        self.expect(TokenKind::OpenBrace, "the trait's methods")?;
        let mut types = Vec::new();
        let mut methods = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at(TokenKind::Type) {
                let doc = self.doc_comment();
                self.advance();
                let Some(name) = self.parse_ident("an associated type name") else {
                    self.recover(RecoveryLevel::Declaration);
                    continue;
                };
                self.expect(TokenKind::Semicolon, "a semicolon after the type")?;
                types.push(AssocType { doc, name });
            } else if self.at(TokenKind::Fn) {
                match self.parse_trait_method(&name, &generics) {
                    Some(id) => methods.push(id),
                    None => self.recover(RecoveryLevel::Declaration),
                }
            } else {
                self.error_missing(
                    "expected a method or `type` declaration",
                    "a method or associated type",
                );
                self.recover(RecoveryLevel::Declaration);
            }
        }
        self.expect(TokenKind::CloseBrace, "a closing brace after the trait")?;
        let span = self.span_from(&start);
        Some(self.ast.traits.alloc(
            Trait {
                doc,
                name,
                generics,
                types,
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
        let doc = self.doc_comment();
        let start = self.current_span();
        self.expect(TokenKind::Fn, "a method declaration")?;
        let name = self.parse_ident("a method name")?;
        let own = self.parse_generics()?;
        let (params, variadic) = self.parse_params(false)?;
        let returns = if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = if self.eat(TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_block()?)
        };
        let this = som_ast::Generic {
            name: Ident {
                name: "Self".into(),
                span: trait_name.span.clone(),
            },
            ty: None,
            bounds: vec![som_ast::Path {
                segments: vec![trait_name.clone()],
            }],
        };
        let generics = std::iter::once(this)
            .chain(trait_generics.iter().cloned())
            .chain(own)
            .collect();
        let span = self.span_from(&start);
        let func = Func {
            doc,
            name,
            generics,
            params,
            returns,
            body,
            variadic,
            link_name: None,
            library: None,
            in_trait: true,
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

    fn parse_use(&mut self) -> Option<Use> {
        self.expect(TokenKind::Use, "a use statement")?;
        let path = self.parse_path("an import path")?;
        self.expect(TokenKind::Semicolon, "a semicolon after the use statement")?;
        Some(Use { path })
    }

    /// `struct Point { x: i32, y: i32 }`, or `extern struct Sqlite3;`: a struct only C
    /// sees inside, used through handles to one.
    fn parse_struct(&mut self) -> Option<Id<Struct>> {
        let doc = self.doc_comment();
        let start = self.current_span();
        let handle = self.eat(TokenKind::Extern);
        self.expect(TokenKind::Struct, "a struct declaration")?;
        let name = self.parse_ident("a struct name")?;
        let generics = self.parse_generics()?;

        let fields = if handle {
            self.expect(TokenKind::Semicolon, "a semicolon after an extern struct")?;
            Vec::new()
        } else {
            self.expect(TokenKind::OpenBrace, "a list of struct fields")?;
            let fields = self.parse_comma_list(TokenKind::CloseBrace, |p| {
                let doc = p.doc_comment();
                let name = p.parse_ident("a struct field name")?;
                p.expect(TokenKind::Colon, "a struct field type")?;
                let ty = p.parse_type()?;
                Some(Field { doc, name, ty })
            })?;
            self.expect(TokenKind::CloseBrace, "a closing brace after struct fields")?;
            fields
        };

        let span = self.span_from(&start);
        Some(self.ast.structs.alloc(
            Struct {
                doc,
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

    /// `enum Shape { Circle(f32), Rect { w: f32, h: f32 }, Empty }`: a type whose values are
    /// each one of its variants.
    fn parse_enum(&mut self) -> Option<Id<Struct>> {
        let doc = self.doc_comment();
        let start = self.current_span();
        self.expect(TokenKind::Enum, "an enum declaration")?;
        let name = self.parse_ident("an enum name")?;
        let generics = self.parse_generics()?;
        self.expect(TokenKind::OpenBrace, "a list of enum variants")?;
        let variants = self.parse_comma_list(TokenKind::CloseBrace, |p| p.parse_variant())?;
        self.expect(TokenKind::CloseBrace, "a closing brace after the variants")?;
        // Variants that hold nothing are just a number.
        let copy = variants.iter().all(|v| v.fields.is_empty());
        let span = self.span_from(&start);
        Some(self.ast.structs.alloc(
            Struct {
                doc,
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
                Some(Field { doc, name, ty })
            })?;
            self.expect(TokenKind::CloseBrace, "a closing brace after the fields")?;
            (VariantKind::Named, fields)
        } else {
            (VariantKind::Unit, Vec::new())
        };
        Some(Variant {
            doc,
            name,
            kind,
            fields,
        })
    }

    /// `fn name<T>(params) -> Ret { body }`
    fn parse_function(&mut self) -> Option<Id<Func>> {
        let doc = self.doc_comment();
        let start = self.current_span();
        self.expect(TokenKind::Fn, "a function declaration")?;
        let name = self.parse_ident("a function name")?;

        let generics = self.parse_generics()?;
        let (params, variadic) = self.parse_params(false)?;
        let returns = if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = Some(self.parse_block()?);

        let span = self.span_from(&start);
        let func = Func {
            doc,
            name,
            generics,
            params,
            returns,
            body,
            variadic,
            link_name: None,
            library: None,
            in_trait: false,
        };
        Some(self.ast.funcs.alloc(func, span))
    }

    /// `<T, N: usize>` declaring generic parameters, if it's there: a type parameter is a
    /// name, a value parameter a name and its type.
    fn parse_generics(&mut self) -> Option<Vec<som_ast::Generic>> {
        if !self.eat(TokenKind::LessThan) {
            return Some(Vec::new());
        }
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
            Some(som_ast::Generic { name, ty, bounds })
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
            let out = self.eat(TokenKind::Out);
            let mutable = !out && self.eat(TokenKind::Mut);
            let name = self.parse_ident("a function parameter name")?;
            let ty = if is_extern {
                self.expect(TokenKind::Colon, "a colon after function parameter name")?;
                Some(self.parse_type()?)
            } else if self.eat(TokenKind::Colon) {
                Some(self.parse_type()?)
            } else {
                None
            };
            params.push(Param {
                name,
                ty,
                out,
                mutable,
            });
            if !self.eat(TokenKind::Comma) {
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
            if self.at(TokenKind::Fn) {
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
        let start = self.current_span();
        self.expect(TokenKind::Fn, "a function declaration")?;
        let name = self.parse_ident("a function name")?;
        let (params, variadic) = self.parse_params(true)?;
        let returns = if self.eat(TokenKind::Arrow) {
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
            name,
            generics: Vec::new(),
            params,
            returns,
            body: None,
            variadic,
            link_name: None,
            library: None,
            in_trait: false,
        };
        Some(self.ast.funcs.alloc(func, span))
    }
}
