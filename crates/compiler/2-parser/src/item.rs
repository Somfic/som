use som_ast::{Expr, Extern, Field, Func, Id, Impl, Item, Param, Struct, Use};
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
            TokenKind::Extern => self.parse_extern().map(Item::Extern),
            TokenKind::Struct => self.parse_struct().map(Item::Struct),
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
            name,
            type_params: Vec::new(),
            params: Vec::new(),
            returns: None,
            body: Some(body),
        };
        let id = self.ast.funcs.alloc(func, span);
        self.add_item(Item::Func(id));
    }

    fn parse_impl(&mut self) -> Option<Impl> {
        self.expect(TokenKind::Impl, "an impl block")?;
        let target = self.parse_ident("a struct name")?;
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

    fn parse_use(&mut self) -> Option<Use> {
        self.expect(TokenKind::Use, "a use statement")?;
        let path = self.parse_path("an import path")?;
        self.expect(TokenKind::Semicolon, "a semicolon after the use statement")?;
        Some(Use { path })
    }

    fn parse_struct(&mut self) -> Option<Id<Struct>> {
        let start = self.current_span();
        self.expect(TokenKind::Struct, "a struct declaration")?;
        let name = self.parse_ident("a struct name")?;

        self.expect(TokenKind::OpenBrace, "a list of struct fields")?;
        let fields = self.parse_comma_list(TokenKind::CloseBrace, |p| {
            let name = p.parse_ident("a struct field name")?;
            p.expect(TokenKind::Colon, "a struct field type")?;
            let ty = p.parse_type()?;
            Some(Field { name, ty })
        })?;
        self.expect(TokenKind::CloseBrace, "a closing brace after struct fields")?;

        let span = self.span_from(&start);
        Some(self.ast.structs.alloc(Struct { name, fields }, span))
    }

    /// `fn name<T>(params) -> Ret { body }`
    fn parse_function(&mut self) -> Option<Id<Func>> {
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

        let params = self.parse_params(false)?;
        let returns = if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = Some(self.parse_block()?);

        let span = self.span_from(&start);
        let func = Func {
            name,
            type_params,
            params,
            returns,
            body,
        };
        Some(self.ast.funcs.alloc(func, span))
    }

    /// `(a: i32, b)`. Extern functions must annotate every parameter.
    fn parse_params(&mut self, require_types: bool) -> Option<Vec<Param>> {
        self.expect(TokenKind::OpenParen, "a list of function parameters")?;
        let params = self.parse_comma_list(TokenKind::CloseParen, |p| {
            let name = p.parse_ident("a function parameter name")?;
            let ty = if require_types {
                p.expect(TokenKind::Colon, "a colon after function parameter name")?;
                Some(p.parse_type()?)
            } else if p.eat(TokenKind::Colon) {
                Some(p.parse_type()?)
            } else {
                None
            };
            Some(Param { name, ty })
        })?;
        self.expect(
            TokenKind::CloseParen,
            "a closing parenthesis after function parameters",
        )?;
        Some(params)
    }

    /// `extern "lib" { fn foo(); }`
    fn parse_extern(&mut self) -> Option<Extern> {
        self.expect(TokenKind::Extern, "an extern block")?;

        let library = if self.at(TokenKind::Text) {
            let text = &self.peek_token().text;
            let library = text[1..text.len() - 1].to_string();
            self.advance();
            Some(library)
        } else {
            None
        };

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

        Some(Extern { library, funcs })
    }

    /// `fn name(params) -> Ret;`
    fn parse_extern_func(&mut self) -> Option<Id<Func>> {
        let start = self.current_span();
        self.expect(TokenKind::Fn, "a function declaration")?;
        let name = self.parse_ident("a function name")?;
        let params = self.parse_params(true)?;
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
            name,
            type_params: Vec::new(),
            params,
            returns,
            body: None,
        };
        Some(self.ast.funcs.alloc(func, span))
    }
}
