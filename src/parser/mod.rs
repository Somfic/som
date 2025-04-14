use std::borrow::Cow;
use std::collections::HashMap;

use crate::ast::{
    Expression, ExpressionValue, FunctionDeclaration, Module, Primitive, Spannable, Statement,
    Typing,
};
use crate::prelude::*;
use crate::tokenizer::{TokenKind, Tokenizer};
use expression::parse_inner_block;
pub use lookup::BindingPower;
use lookup::Lookup;
use miette::MietteDiagnostic;

mod expression;
mod lookup;
mod module;
mod statement;
mod typing;

pub struct Parser<'ast> {
    errors: Vec<MietteDiagnostic>,
    tokens: Tokenizer<'ast>,
    lookup: Lookup<'ast>,
}

impl<'ast> Parser<'ast> {
    pub fn new(source_code: &'ast str) -> Self {
        Self {
            errors: Vec::new(),
            tokens: Tokenizer::new(source_code),
            lookup: Lookup::default(),
        }
    }

    fn report_error(&mut self, error: MietteDiagnostic) {
        self.errors.push(error);
    }

    fn report_errors(&mut self, errors: &[MietteDiagnostic]) {
        self.errors.extend_from_slice(errors);
    }

    pub fn parse(&mut self) -> ParserResult<Vec<Module<'ast>>> {
        let mut modules = vec![];

        let entry_module = self.parse_entry_module()?;
        modules.push(entry_module);

        // while let Some(token) = self.tokens.peek() {
        //     match token {
        //         Ok(_) => {
        //             let module = self.parse_module()?;
        //             modules.push(module);
        //         }
        //         Err(err) => {
        //             self.errors.extend(err.to_vec());
        //             self.tokens.next();
        //         }
        //     }
        // }

        if self.errors.is_empty() {
            Ok(modules)
        } else {
            Err(self.errors.clone())
        }
    }

    fn parse_entry_module(&mut self) -> ParserResult<Module<'ast>> {
        let expression = parse_inner_block(self)?;

        let main_function = FunctionDeclaration {
            name: Cow::Borrowed("0"),
            span: expression.span,
            body: expression,
            parameters: Vec::new(),
            explicit_return_type: None,
        };

        Ok(Module {
            functions: vec![main_function],
            intrinsic_functions: vec![],
        })
    }

    fn parse_module(&mut self) -> ParserResult<Module<'ast>> {
        let mut functions = vec![];
        let mut intrinsic_functions = vec![];

        while let Some(Ok(token)) = self.tokens.peek() {
            match token.kind {
                TokenKind::Intrinsic => {
                    intrinsic_functions.push(module::parse_module_intrinsic_function(self)?);
                }
                TokenKind::Function => {
                    functions.push(module::parse_module_function(self)?);
                }
                _ => {
                    return Err(vec![miette::diagnostic! {
                        labels = vec![token.label("expected a function or intrinsic function here")],
                        help = format!("{} cannot be parsed as a function", token.kind),
                        "expected a function, found {}", token.kind
                    }]);
                }
            }
        }

        Ok(Module {
            functions,
            intrinsic_functions,
        })
    }

    pub(crate) fn parse_expression(&mut self, bp: BindingPower) -> ParserResult<Expression<'ast>> {
        let token = match self.tokens.peek().as_ref() {
            Some(Ok(token)) => token,
            Some(Err(err)) => return Err(err.to_vec()),
            None => {
                // TODO: Use report_error and return some sort of phantom expression so that
                //  we can handle multiple errors in the parse pass
                return Err(vec![miette::diagnostic! {
                    help = "expected an expression",
                    "expected an expression"
                }]);
            }
        };

        let handler =
            self.lookup
                .expression_lookup
                .get(&token.kind)
                .ok_or(vec![miette::diagnostic! {
                    labels = vec![token.label("expected an expression here")],
                    help = format!("{} cannot be parsed as an expression", token.kind),
                    "expected an new expression, found {}", token.kind
                }])?;
        let mut lhs = handler(self)?;

        let mut next_token = self.tokens.peek();

        while let Some(token) = next_token {
            let token = match token {
                Ok(token) => token,
                Err(err) => return Err(err.to_vec()),
            };

            let token_binding_power = {
                let binding_power_lookup = self.lookup.binding_power_lookup.clone();
                binding_power_lookup
                    .get(&token.kind)
                    .unwrap_or(&BindingPower::None)
                    .clone()
            };

            if bp >= token_binding_power {
                break;
            }

            let handler = match self.lookup.left_expression_lookup.get(&token.kind) {
                Some(handler) => handler,
                None => break,
            };

            self.tokens.next();

            lhs = handler(self, lhs, token_binding_power)?;

            next_token = self.tokens.peek();
        }

        Ok(lhs)
    }

    pub(crate) fn parse_statement(
        &mut self,
        require_semicolon: bool,
    ) -> ParserResult<Statement<'ast>> {
        let token = match self.tokens.peek().as_ref() {
            Some(Ok(token)) => token,
            Some(Err(err)) => return Err(err.to_vec()),
            None => {
                return Err(vec![miette::diagnostic! {
                    help = "expected a statement",
                    "expected a statement"
                }]);
            }
        };

        let statement = match self.lookup.statement_lookup.get(&token.kind) {
            Some(handler) => handler(self)?,
            None => {
                // parse expression
                let expression = self.parse_expression(BindingPower::None)?;
                Statement::expression(expression.span, expression)
            }
        };

        if require_semicolon {
            self.tokens
                .expect(TokenKind::Semicolon, "expected a closing semicolon")?;
        }

        Ok(statement)
    }

    pub(crate) fn parse_typing(&mut self, bp: BindingPower) -> ParserResult<Typing<'ast>> {
        let token = match self.tokens.peek().as_ref() {
            Some(Ok(token)) => token,
            Some(Err(err)) => return Err(err.to_vec()),
            None => {
                return Err(vec![miette::diagnostic! {
                    help = "expected a type",
                    "expected a type"
                }]);
            }
        };

        let handler =
            self.lookup
                .typing_lookup
                .get(&token.kind)
                .ok_or(vec![miette::diagnostic! {
                    labels = vec![token.label("expected a type here")],
                    help = format!("{} cannot be parsed as a type", token.kind),
                    "expected a type, found {}", token.kind
                }])?;
        let mut lhs = handler(self)?;

        let mut next_token = self.tokens.peek();

        while let Some(token) = next_token {
            let token = match token {
                Ok(token) => token,
                Err(err) => return Err(err.to_vec()),
            };

            let token_binding_power = {
                let binding_power_lookup = self.lookup.binding_power_lookup.clone();
                binding_power_lookup
                    .get(&token.kind)
                    .unwrap_or(&BindingPower::None)
                    .clone()
            };

            if bp >= token_binding_power {
                break;
            }

            let handler = match self.lookup.left_typing_lookup.get(&token.kind) {
                Some(handler) => handler,
                None => break,
            };

            self.tokens.next();

            lhs = handler(self, lhs, token_binding_power)?;

            next_token = self.tokens.peek();
        }

        Ok(lhs)
    }
}
