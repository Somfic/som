use crate::ast::Expression;
use crate::prelude::*;
use crate::tokenizer::Tokenizer;
pub use lookup::BindingPower;
use lookup::Lookup;

mod expression;
mod lookup;

pub struct Parser<'ast> {
    pub tokens: Tokenizer<'ast>,
    pub lookup: Lookup<'ast>,
}

impl<'ast> Parser<'ast> {
    pub fn new(source_code: &'ast str) -> Self {
        Self {
            tokens: Tokenizer::new(source_code),
            lookup: Lookup::default(),
        }
    }

    pub fn parse_expression(&mut self, bp: lookup::BindingPower) -> Result<Expression<'ast>> {
        let token = match self.tokens.peek().as_ref() {
            Some(Ok(token)) => token,
            Some(Err(err)) => return Err(err.to_vec()),
            None => {
                return Err(vec![miette::diagnostic! {
                    help = "expected an expression",
                    "expected an expression"
                }])
            }
        };

        let handler =
            self.lookup
                .expression_lookup
                .get(&token.kind)
                .ok_or(vec![miette::diagnostic! {
                    labels = vec![token.label("expected an expression")],
                    help = format!("{} is not an expression", token.kind),
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

            if bp > token_binding_power {
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
}
