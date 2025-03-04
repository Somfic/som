use crate::ast::{Expression, Module, Statement, Typing};
use crate::prelude::*;
use crate::tokenizer::{TokenKind, Tokenizer};
pub use lookup::BindingPower;
use lookup::Lookup;

mod expression;
mod lookup;
mod module;
mod statement;
mod typing;

pub struct Parser<'ast> {
    tokens: Tokenizer<'ast>,
    lookup: Lookup<'ast>,
}

impl<'ast> Parser<'ast> {
    pub fn new(source_code: &'ast str) -> Self {
        Self {
            tokens: Tokenizer::new(source_code),
            lookup: Lookup::default(),
        }
    }

    pub fn parse(&mut self) -> ParserResult<Vec<Module<'ast>>> {
        let mut modules = vec![];
        let mut errors = Diagnostics::new();

        while self.tokens.peek().is_some() {
            let module = match self.parse_module(&mut errors) {
                Ok(module) => module,
                Err(error) => {
                    errors.extend(error);

                    // keep consuming tokens until we can succesfully parse the next module with self.module_parse
                    while self.tokens.next().is_some() {
                        match self.parse_module(&mut Diagnostics::new()) {
                            Ok(module) => {
                                println!("recovered from error");
                                modules.push(module);
                                break;
                            }
                            Err(_) => continue,
                        }
                    }
                    continue;
                }
            };
            modules.push(module);
        }

        if errors.is_empty() {
            Ok(modules)
        } else {
            Err(errors)
        }
    }

    fn parse_module(&mut self, errors: &mut Diagnostics) -> ParserResult<Module<'ast>> {
        let mut functions = vec![];

        while self.tokens.peek().is_some() {
            let function = module::parse_function(self, errors)?;
            functions.push(function);
        }

        // make sure there is a main function
        if functions.iter().all(|function| function.name != "main") {
            errors.add(miette::diagnostic! {
                help = "add a main function",
                "missing main function"
            });
        }

        // set the main function to return an integer
        for function in &mut functions {
            if function.name == "main" {
                if function.return_type.is_none() {
                    function.return_type = Some(Typing::integer(&function.span));
                } else {
                    errors.add(miette::diagnostic! {
                        help = "remove the return type",
                        "main function must return an integer"
                    });
                }
            }
        }

        Ok(Module { functions })
    }

    pub(crate) fn parse_expression(&mut self, bp: BindingPower) -> ParserResult<Expression<'ast>> {
        let token = match self.tokens.peek() {
            Some(token) => token,
            _ => {
                return Err(Diagnostics::with(miette::diagnostic! {
                    help = "expected an expression",
                    "expected an expression"
                }));
            }
        };

        let handler = self
            .lookup
            .expression_lookup
            .get(&token.kind)
            .ok_or(Diagnostics::with(miette::diagnostic! {
                labels = vec![token.label("expected an expression here")],
                help = format!("{} cannot be parsed as an expression", token.kind),
                "expected an new expression, found {}", token.kind
            }))?;

        let mut lhs = handler(self)?;

        let mut next_token = self.tokens.peek();

        while let Some(token) = next_token {
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
        let token = match self.tokens.peek() {
            Some(token) => token,
            _ => {
                return Err(Diagnostics::with(miette::diagnostic! {
                    help = "expected a statement",
                    "expected a statement"
                }));
            }
        };

        match self.lookup.statement_lookup.get(&token.kind) {
            Some(handler) => handler(self),
            None => {
                // parse expression
                let expression = self.parse_expression(BindingPower::None)?;

                if require_semicolon {
                    self.tokens
                        .expect(TokenKind::Semicolon, "expected a closing semicolon")?;
                }

                Ok(Statement::expression(expression.span, expression))
            }
        }
    }

    pub(crate) fn parse_typing(&mut self, bp: BindingPower) -> ParserResult<Typing<'ast>> {
        let token = match self.tokens.peek() {
            Some(token) => token,
            _ => {
                return Err(Diagnostics::with(miette::diagnostic! {
                    help = "expected a type",
                    "expected a type"
                }));
            }
        };

        let handler = self
            .lookup
            .typing_lookup
            .get(&token.kind)
            .ok_or(Diagnostics::with(miette::diagnostic! {
                labels = vec![token.label("expected a type here")],
                help = format!("{} cannot be parsed as a type", token.kind),
                "expected an new type, found {}", token.kind
            }))?;

        let mut lhs = handler(self)?;

        let mut next_token = self.tokens.peek();

        while let Some(token) = next_token {
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
