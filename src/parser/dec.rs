use crate::ast::{DecId, FuncDec, FuncParam, Module, Type};
use crate::lexer::TokenKind;
use crate::parser::Parser;

impl<'src> Parser<'src> {
    pub(super) fn parse_program(&mut self) {
        let mut func_ids = Vec::new();

        while !self.at_eof() {
            if self.at(TokenKind::FnKw) {
                if let Some(func_id) = self.parse_func_dec() {
                    func_ids.push(DecId::Func(func_id));
                }
            } else {
                // Unexpected token at top level
                self.error(vec![TokenKind::FnKw]);
                self.advance(); // Skip to recover
            }
        }

        self.ast.mods.push(Module {
            name: "main".into(),
            decs: func_ids,
        });
    }

    fn parse_func_dec(&mut self) -> Option<crate::ast::FuncId> {
        let start_span = self.peek_span();

        // fn
        self.expect(TokenKind::FnKw)?;

        // name
        let (name, _) = self.parse_ident()?;

        // (params)
        self.expect(TokenKind::LeftParen)?;

        let mut parameters = Vec::new();
        if !self.at(TokenKind::RightParen) {
            loop {
                if let Some(param) = self.parse_func_param() {
                    parameters.push(param);
                }

                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(TokenKind::RightParen)?;

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
        let span = start_span.merge(end_span);

        let func = FuncDec {
            name,
            parameters,
            return_type,
            return_type_id,
            body,
        };

        Some(self.ast.alloc_func_with_span(func, span))
    }

    fn parse_func_param(&mut self) -> Option<FuncParam> {
        let (name, _) = self.parse_ident()?;

        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        Some(FuncParam { name, ty })
    }

    pub(super) fn parse_type(&mut self) -> Option<Type> {
        if self.at(TokenKind::Ident) {
            let token = self.peek_token();
            let text = token.text;
            self.advance();

            match text {
                "i32" => Some(Type::I32),
                "bool" => Some(Type::Bool),
                "unit" => Some(Type::Unit),
                _ => {
                    // Unknown type - could add custom types later
                    self.errors.push(crate::parser::ParseError::new(
                        vec![],
                        TokenKind::Ident,
                        self.previous_span(),
                    ));
                    None
                }
            }
        } else {
            self.error(vec![TokenKind::Ident]);
            None
        }
    }
}
