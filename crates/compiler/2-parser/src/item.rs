use som_ast::{Expr, Extern, Field, Func, Id, Impl, Item, Param, Struct, Use};
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

            match self.parse_stmt_or_expr() {
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) if self.at_eof() => value = Some(expr),
                StmtOrExpr::Expr(_) => {
                    self.error_missing("expected ;", "a semicolon after the expression");
                    break;
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
            type_params: Vec::new(),
            params: Vec::new(),
            returns: None,
            body: Some(body),
            variadic: false,
            link_name: None,
            library: None,
        };
        let id = self.ast.funcs.alloc(func, span);
        self.add_item(Item::Func(id));
    }

    fn parse_impl(&mut self) -> Option<Impl> {
        self.expect(TokenKind::Impl, "an impl block")?;
        let target = self.parse_path("a struct name")?;
        self.expect(TokenKind::OpenBrace, "an impl block")?;

        let mut methods = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at(TokenKind::Fn) {
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
        Some(Impl { target, methods })
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
                fields,
                // A handle is just a pointer, so copying it copies nothing C owns.
                copy: handle,
                handle,
            },
            span,
        ))
    }

    /// `fn name<T>(params) -> Ret { body }`
    fn parse_function(&mut self) -> Option<Id<Func>> {
        let doc = self.doc_comment();
        let start = self.current_span();
        self.expect(TokenKind::Fn, "a function declaration")?;
        let name = self.parse_ident("a function name")?;

        let type_params = if self.eat(TokenKind::LessThan) {
            let params = self.parse_comma_list(TokenKind::GreaterThan, |p| {
                p.parse_ident("a type parameter name")
            })?;
            self.expect(
                TokenKind::GreaterThan,
                "a closing angle bracket for type parameters",
            )?;
            params
        } else {
            Vec::new()
        };

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
            type_params,
            params,
            returns,
            body,
            variadic,
            link_name: None,
            library: None,
        };
        Some(self.ast.funcs.alloc(func, span))
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
            type_params: Vec::new(),
            params,
            returns,
            body: None,
            variadic,
            link_name: None,
            library: None,
        };
        Some(self.ast.funcs.alloc(func, span))
    }
}
