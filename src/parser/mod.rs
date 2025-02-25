use crate::ast::{Expression, Statement};
use crate::prelude::*;
use crate::tokenizer::{TokenKind, Tokenizer};
pub use lookup::BindingPower;
use lookup::Lookup;
use miette::MietteDiagnostic;

mod expression;
mod lookup;
mod statement;

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

    pub fn parse(&mut self) -> ParserResult<Vec<Statement<'ast>>> {
        let mut statetements = vec![];

        while let Some(token) = self.tokens.peek() {
            match token {
                Ok(_) => {
                    let statement = self.parse_statement(true)?;
                    statetements.push(statement);
                }
                Err(err) => {
                    self.errors.extend(err.to_vec());
                    self.tokens.next();
                }
            }
        }

        if self.errors.is_empty() {
            Ok(statetements)
        } else {
            Err(self.errors.clone())
        }
    }

    fn parse_expression(&mut self, bp: lookup::BindingPower) -> ParserResult<Expression<'ast>> {
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
                    "expected an expression, found {}", token.kind
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

    fn parse_statement(&mut self, require_semicolon: bool) -> ParserResult<Statement<'ast>> {
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
}
