use crate::{
    ExternBlock, ExternFunc, Func, FuncParam, FuncTypeParam, Struct, StructField, Use,
    arena::Id,
    lexer::TokenKind,
    parser::{Parser, RecoveryLevel},
};

impl<'src> Parser<'src> {
    pub fn parse_program(&mut self) {
        while !self.at_eof() {
            // declarations
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
                _ => {
                    self.error(format!(
                        "expected `fn`, `extern`, `use`, or `struct`, found {:?}",
                        self.peek()
                    ));
                    self.recover(RecoveryLevel::Declaration);
                }
            }
        }
    }

    fn parse_use(&mut self) -> Option<Id<Use>> {
        let start = self.current_span();
        self.expect(TokenKind::Use);

        let path = self.parse_separated(TokenKind::DoubleColon, TokenKind::Semicolon, |p| {
            p.parse_ident()
        })?;

        self.expect(TokenKind::Semicolon);

        let span = start.merge(&self.previous_span());

        Some(self.builder.alloc_use(Use { path }, span))
    }

    fn parse_struct(&mut self) -> Option<Id<Struct>> {
        let start = self.current_span();
        self.expect(TokenKind::Struct)?;

        let name = self.parse_ident()?;

        self.expect(TokenKind::OpenBrace)?;

        let fields = self.parse_separated(TokenKind::Comma, TokenKind::CloseBrace, |p| {
            p.parse_struct_field()
        })?;

        self.expect(TokenKind::CloseBrace)?;

        let span = start.merge(&self.previous_span());
        Some(self.builder.alloc_struct(Struct { name, fields }, span))
    }

    fn parse_struct_field(&mut self) -> Option<StructField> {
        let name = self.parse_ident()?;

        self.expect(TokenKind::Colon)?;
        let ty_span = self.current_span();
        let ty = self.parse_type()?;
        let type_id = self.builder.alloc_type_span(ty_span);

        Some(StructField { name, ty, type_id })
    }

    /// Parse a function declaration: `fn name<T>(params) -> RetType { body }`
    fn parse_function(&mut self) -> Option<Id<Func>> {
        let start = self.current_span();
        self.expect(TokenKind::Fn)?;

        let name = self.parse_ident()?;

        // Parse optional type parameters: <T, U>
        let type_parameters = if self.eat(TokenKind::LessThan) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        // Parse parameters: (x: i32, y: bool)
        self.expect(TokenKind::OpenParen)?;
        let parameters = self.parse_func_params()?;
        self.expect(TokenKind::CloseParen)?;

        // Parse optional return type: -> i32
        let (return_type, return_type_id) = if self.eat(TokenKind::Arrow) {
            let ty_span = self.current_span();
            let ty = self.parse_type()?;
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
            let name = self.parse_ident()?;
            params.push(FuncTypeParam { name });

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::GreaterThan) {
                    break; // Trailing comma
                }
                let name = self.parse_ident()?;
                params.push(FuncTypeParam { name });
            }
        }

        self.expect(TokenKind::GreaterThan)?;
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
        let name = self.parse_ident()?;

        let (ty, type_id) = if self.eat(TokenKind::Colon) {
            let ty_span = self.current_span();
            let ty = self.parse_type()?;
            let ty_id = self.builder.alloc_type_span(ty_span);
            (Some(ty), Some(ty_id))
        } else {
            (None, None)
        };

        Some(FuncParam { name, ty, type_id })
    }

    /// Parse an extern block: `extern "lib" { fn foo(); }`
    fn parse_extern_block(&mut self) -> Option<ExternBlock> {
        self.expect(TokenKind::Extern)?;

        // Parse optional library name: "SDL2"
        let library = if self.at(TokenKind::Text) {
            let text = self.peek_token().text;
            // Remove surrounding quotes
            let unquoted = &text[1..text.len() - 1];
            self.advance();
            Some(unquoted.to_string())
        } else {
            None
        };

        self.expect(TokenKind::OpenBrace)?;

        let mut functions = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at(TokenKind::Fn) {
                if let Some(func) = self.parse_extern_func() {
                    functions.push(func);
                } else {
                    self.recover(RecoveryLevel::Declaration);
                }
            } else {
                self.error("expected `fn` in extern block".into());
                self.recover(RecoveryLevel::Declaration);
            }
        }

        self.expect(TokenKind::CloseBrace)?;

        Some(ExternBlock { library, functions })
    }

    /// Parse an extern function declaration: `fn name(params) -> RetType;`
    fn parse_extern_func(&mut self) -> Option<Id<ExternFunc>> {
        let start = self.current_span();
        self.expect(TokenKind::Fn)?;

        let name = self.parse_ident()?;

        // Parse parameters
        self.expect(TokenKind::OpenParen)?;
        let parameters = self.parse_extern_func_params()?;
        self.expect(TokenKind::CloseParen)?;

        // Parse optional return type
        let return_type = if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(TokenKind::Semicolon)?;

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
        let name = self.parse_ident()?;

        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;

        Some(FuncParam {
            name,
            ty: Some(ty),
            type_id: None,
        })
    }
}
