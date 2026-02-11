use crate::arena::Id;
use crate::ast::{Decl, Func, FuncParam, Module};
use crate::lexer::TokenKind;
use crate::parser::Parser;
use crate::{ExternBlock, ExternFunc, FuncTypeParam};

impl<'src> Parser<'src> {
    pub(super) fn parse_program(&mut self) {
        let mut declarations = Vec::new();

        while !self.at_eof() {
            if self.at(TokenKind::Fn) {
                if let Some(func_id) = self.parse_func_dec() {
                    declarations.push(Decl::Func(func_id));
                }
            } else if self.at(TokenKind::Extern) {
                if let Some(extern_block) = self.parse_extern_block() {
                    declarations.push(Decl::ExternBlock(extern_block));
                }
            } else {
                // Unexpected token at top level
                self.error(vec![TokenKind::Fn, TokenKind::Extern]);
                self.advance(); // Skip to recover
            }
        }

        self.ast.mods.push(Module {
            name: "main".into(),
            decs: declarations,
        });
    }

    fn parse_func_dec(&mut self) -> Option<Id<Func>> {
        let start_span = self.peek_span();

        // fn
        self.expect(TokenKind::Fn)?;

        // name
        let (name, _) = self.parse_ident()?;

        // type parameters
        let mut type_parameters = vec![];
        if self.eat(TokenKind::LessThan) {
            loop {
                let (param_name, _) = self.parse_ident()?;
                type_parameters.push(FuncTypeParam { name: param_name });

                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::GreaterThan)?;
        }

        // parameters
        self.expect(TokenKind::OpenParen)?;

        let mut parameters = Vec::new();
        if !self.at(TokenKind::CloseParen) {
            loop {
                if let Some(param) = self.parse_func_param() {
                    parameters.push(param);
                }

                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(TokenKind::CloseParen)?;

        // Optional return type
        let (return_type, return_type_id) = if self.eat(TokenKind::Arrow) {
            let type_span = self.peek_span();
            let ty = self.parse_type()?;
            let type_id = self.ast.alloc_type_with_span(type_span);
            (Some(ty), Some(type_id))
        } else {
            (None, None)
        };

        // Body (block expression)
        let body = self.parse_block()?;

        let end_span = self.previous_span();
        let span = start_span.merge(&end_span);

        let func = Func {
            name,
            type_parameters,
            parameters,
            return_type,
            return_type_id,
            body,
        };

        Some(self.ast.alloc_func_with_span(func, span))
    }

    fn parse_func_param(&mut self) -> Option<FuncParam> {
        let (name, _) = self.parse_ident()?;

        let (ty, type_id) = if self.eat(TokenKind::Colon) {
            let type_span = self.peek_span();
            let parsed_ty = self.parse_type()?;
            let tid = self.ast.alloc_type_with_span(type_span);
            (Some(parsed_ty), Some(tid))
        } else {
            (None, None)
        };

        Some(FuncParam { name, ty, type_id })
    }

    fn parse_extern_block(&mut self) -> Option<ExternBlock> {
        self.expect(TokenKind::Extern)?;

        let library = if self.at(TokenKind::Text) {
            let token = self.peek_token();
            let value = token.text.to_string();
            self.advance(); // consume the library string
            Some(value)
        } else {
            None
        };

        self.expect(TokenKind::OpenBrace)?;

        let mut functions = Vec::new();
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if let Some(func) = self.parse_extern_func() {
                functions.push(
                    self.ast
                        .alloc_extern_func_with_span(func, self.previous_span()),
                );
            } else {
                // Skip to next semicolon to recover
                while !self.at(TokenKind::Semicolon)
                    && !self.at(TokenKind::CloseBrace)
                    && !self.at_eof()
                {
                    self.advance();
                }
                self.eat(TokenKind::Semicolon); // Skip the semicolon if we stopped at one
            }
        }

        self.expect(TokenKind::CloseBrace)?;

        Some(ExternBlock { library, functions })
    }

    fn parse_extern_func(&mut self) -> Option<ExternFunc> {
        let start_span = self.peek_span();

        self.expect(TokenKind::Fn)?;

        // name
        let (name, _) = self.parse_ident()?;

        // parameters
        self.expect(TokenKind::OpenParen)?;

        let mut parameters = Vec::new();
        if !self.at(TokenKind::CloseParen) {
            loop {
                if let Some(param) = self.parse_func_param() {
                    parameters.push(param);
                }

                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(TokenKind::CloseParen)?;

        // Optional return type
        let return_type = if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(TokenKind::Semicolon)?;

        let end_span = self.previous_span();
        let span = start_span.merge(&end_span);

        Some(ExternFunc {
            name,
            parameters,
            return_type,
        })
    }
}
