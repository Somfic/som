use crate::{
    Expr, ExternBlock, ExternFunc, Func, FuncParam, FuncTypeParam, Struct, StructField, Use,
    arena::Id,
    lexer::TokenKind,
    parser::{Parser, RecoveryLevel, StmtOrExpr},
};

impl Parser<'_> {
    fn is_declaration(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Fn | TokenKind::Extern | TokenKind::Struct | TokenKind::Use
        )
    }

    fn parse_declaration(&mut self) {
        match self.peek() {
            TokenKind::Fn => {
                if let Some(func_id) = self.parse_function() {
                    self.builder.add_func(func_id);
                } else {
                    self.recover(RecoveryLevel::Declaration);
                }
            }
            TokenKind::Extern => {
                if let Some(block) = self.parse_extern_block() {
                    self.builder.add_extern_block(block);
                } else {
                    self.recover(RecoveryLevel::Declaration);
                }
            }
            TokenKind::Struct => {
                if let Some(struct_id) = self.parse_struct() {
                    self.builder.add_struct(struct_id);
                } else {
                    self.recover(RecoveryLevel::Declaration);
                }
            }
            TokenKind::Use => {
                if let Some(use_id) = self.parse_use() {
                    self.builder.add_use(use_id);
                } else {
                    self.recover(RecoveryLevel::Declaration);
                }
            }
            _ => unreachable!("caller checks is_declaration"),
        }
    }

    pub fn parse_program(&mut self) {
        // Parse leading declarations
        while !self.at_eof() && self.is_declaration() {
            self.parse_declaration();
        }

        // If there's remaining code, treat it as the body of an implicit main function
        if !self.at_eof() {
            self.parse_implicit_main();
        }
    }

    /// Parse remaining top-level statements as an implicit `fn main() -> i32 { ... }`
    fn parse_implicit_main(&mut self) {
        let start = self.current_span();
        let mut stmts = Vec::new();
        let mut value = None;

        while !self.at_eof() {
            // Allow interleaved declarations inside implicit main
            if self.is_declaration() {
                self.parse_declaration();
                continue;
            }

            match self.parse_stmt_or_expr() {
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) => {
                    if self.at_eof() {
                        value = Some(expr);
                    } else {
                        self.error_missing("expected ;", "a semicolon after the expression");
                        break;
                    }
                }
                StmtOrExpr::Error => {
                    self.recover(RecoveryLevel::Statement);
                }
            }
        }

        let span = start.merge(&self.previous_span());
        let body = self
            .builder
            .alloc_expr(Expr::Block { stmts, value }, span.clone());

        let name = self.builder.make_ident("main");
        let func_id = self.builder.alloc_func(
            Func {
                name,
                type_parameters: Vec::new(),
                parameters: Vec::new(),
                return_type: None,
                return_type_id: None,
                body,
            },
            span,
        );
        self.builder.add_func(func_id);
    }

    fn parse_use(&mut self) -> Option<Id<Use>> {
        let start = self.current_span();
        self.expect_closing(TokenKind::Use, "a use statement");

        let path_start = self.current_span();
        let path = self.parse_path("an import path")?;
        let path_span = path_start.merge(&self.previous_span());

        self.expect_closing(TokenKind::Semicolon, "a semicolon after the use statement")?;

        let span = start.merge(&self.previous_span());

        Some(self.builder.alloc_use(Use { path, path_span }, span))
    }

    fn parse_struct(&mut self) -> Option<Id<Struct>> {
        let start = self.current_span();
        self.expect_closing(TokenKind::Struct, "a struct declaration")?;

        let name = self.parse_ident("a struct name")?;

        self.expect_closing(TokenKind::OpenBrace, "a list of struct fields")?;

        let fields = self.parse_separated(
            TokenKind::Comma,
            TokenKind::CloseBrace,
            |p| p.parse_struct_field(),
            "a struct field",
        )?;

        self.expect_closing(TokenKind::CloseBrace, "a closing brace after struct fields")?;

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_struct(Struct { name, fields }, span))
    }

    fn parse_struct_field(&mut self) -> Option<StructField> {
        let name = self.parse_ident("a struct field name")?;

        self.expect_closing(TokenKind::Colon, "a struct field type")?;
        let ty_start = self.current_span();
        let ty = self.parse_type()?;
        let ty_span = ty_start.merge(&self.previous_span());
        let type_id = self.builder.alloc_type_span(ty_span);

        Some(StructField { name, ty, type_id })
    }

    /// Parse a function declaration: `fn name<T>(params) -> RetType { body }`
    fn parse_function(&mut self) -> Option<Id<Func>> {
        let start = self.current_span();
        self.expect_closing(TokenKind::Fn, "a function declaration")?;

        let name = self.parse_ident("a function name")?;

        // Parse optional type parameters: <T, U>
        let type_parameters = if self.eat(TokenKind::LessThan) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        // Parse parameters: (x: i32, y: bool)
        self.expect_closing(TokenKind::OpenParen, "a list of function parameters")?;
        let parameters = self.parse_func_params()?;
        self.expect_closing(
            TokenKind::CloseParen,
            "a closing parenthesis after function parameters",
        )?;

        // Parse optional return type: -> i32
        let (return_type, return_type_id) = if self.eat(TokenKind::Arrow) {
            let ty_start = self.current_span();
            let ty = self.parse_type()?;
            let ty_span = ty_start.merge(&self.previous_span());
            let ty_id = self.builder.alloc_type_span(ty_span);
            (Some(ty), Some(ty_id))
        } else {
            (None, None)
        };

        // Parse body
        let body = self.parse_block()?;

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_func(
            Func {
                name,
                type_parameters,
                parameters,
                return_type,
                return_type_id,
                body,
            },
            span,
        ))
    }

    /// Parse type parameters: T, U, V
    fn parse_type_params(&mut self) -> Option<Vec<FuncTypeParam>> {
        let mut params = Vec::new();

        if !self.at(TokenKind::GreaterThan) {
            let name = self.parse_ident("a type parameter name")?;
            params.push(FuncTypeParam { name });

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::GreaterThan) {
                    break; // Trailing comma
                }
                let name = self.parse_ident("a type parameter name")?;
                params.push(FuncTypeParam { name });
            }
        }

        self.expect_closing(
            TokenKind::GreaterThan,
            "a closing angle bracket for type parameters",
        )?;
        Some(params)
    }

    /// Parse function parameters: x: i32, y: bool
    fn parse_func_params(&mut self) -> Option<Vec<FuncParam>> {
        let mut params = Vec::new();

        if !self.at(TokenKind::CloseParen) {
            params.push(self.parse_func_param()?);

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::CloseParen) {
                    break; // Trailing comma
                }
                params.push(self.parse_func_param()?);
            }
        }

        Some(params)
    }

    /// Parse a single function parameter: name: Type
    fn parse_func_param(&mut self) -> Option<FuncParam> {
        let name = self.parse_ident("a function parameter name")?;

        let (ty, type_id) = if self.eat(TokenKind::Colon) {
            let ty_start = self.current_span();
            let ty = self.parse_type()?;
            let ty_span = ty_start.merge(&self.previous_span());
            let ty_id = self.builder.alloc_type_span(ty_span);
            (Some(ty), Some(ty_id))
        } else {
            (None, None)
        };

        Some(FuncParam { name, ty, type_id })
    }

    /// Parse an extern block: `extern "lib" { fn foo(); }`
    fn parse_extern_block(&mut self) -> Option<ExternBlock> {
        self.expect_closing(TokenKind::Extern, "an extern block")?;

        // Parse optional library name: "SDL2"
        let library = if self.at(TokenKind::Text) {
            let text = self.peek_token().text.clone();
            // Remove surrounding quotes
            let unquoted = &text[1..text.len() - 1];
            let lib = unquoted.to_string();
            self.advance();
            Some(lib)
        } else {
            None
        };

        self.expect_closing(
            TokenKind::OpenBrace,
            "a list of extern function declarations",
        )?;

        let mut functions = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at(TokenKind::Fn) {
                if let Some(func) = self.parse_extern_func() {
                    functions.push(func);
                } else {
                    self.recover(RecoveryLevel::Declaration);
                }
            } else {
                self.error_missing(
                    "expected extern function declaration",
                    "an extern function declaration",
                );
                self.recover(RecoveryLevel::Declaration);
            }
        }

        self.expect_closing(TokenKind::CloseBrace, "a closing brace after extern block")?;

        Some(ExternBlock { library, functions })
    }

    /// Parse an extern function declaration: `fn name(params) -> RetType;`
    fn parse_extern_func(&mut self) -> Option<Id<ExternFunc>> {
        let start = self.current_span();
        self.expect_closing(TokenKind::Fn, "an function declaration")?;

        let name = self.parse_ident("a function name")?;

        // Parse parameters
        self.expect_closing(TokenKind::OpenParen, "a list of function parameters")?;
        let parameters = self.parse_extern_func_params()?;
        self.expect_closing(
            TokenKind::CloseParen,
            "a closing parenthesis after function parameters",
        )?;

        // Parse optional return type
        let return_type = if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect_closing(
            TokenKind::Semicolon,
            "a semicolon after extern function declaration",
        )?;

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_extern_func(
            ExternFunc {
                name,
                parameters,
                return_type,
            },
            span,
        ))
    }

    /// Parse extern function parameters (no type_id needed)
    fn parse_extern_func_params(&mut self) -> Option<Vec<FuncParam>> {
        let mut params = Vec::new();

        if !self.at(TokenKind::CloseParen) {
            params.push(self.parse_extern_func_param()?);

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::CloseParen) {
                    break;
                }
                params.push(self.parse_extern_func_param()?);
            }
        }

        Some(params)
    }

    /// Parse an extern function parameter
    fn parse_extern_func_param(&mut self) -> Option<FuncParam> {
        let name = self.parse_ident("a function parameter name")?;

        self.expect_closing(TokenKind::Colon, "a colon after function parameter name")?;
        let ty = self.parse_type()?;

        Some(FuncParam {
            name,
            ty: Some(ty),
            type_id: None,
        })
    }
}
