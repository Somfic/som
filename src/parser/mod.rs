use crate::prelude::*;

pub mod lookup;

pub struct Parser<'source> {
    lexer: Lexer<'source>,
    lookup: Lookup,
}

impl<'source> Parser<'source> {
    pub fn new(lexer: Lexer<'source>) -> Self {
        Self {
            lexer,
            lookup: Lookup::default(),
        }
    }

    pub fn expect(&mut self, expected: TokenKind) -> Result<Token> {
        match self.lexer.next() {
            Some(Ok(token)) if expected == token.kind => Ok(token),
            Some(Ok(token)) => Err(parser_unexpected_token(&token, &expected).into()),
            Some(Err(e)) => Err(e),
            None => Ok(Token {
                kind: TokenKind::EOF,
                value: TokenValue::None,
                span: SourceSpan::new(self.lexer.byte_offset.into(), 0),
                original: "".into(),
            }),
        }
    }

    pub fn parse_expression(&mut self, bp: BindingPower) -> Result<Expression> {
        let token = match self.lexer.peek().as_ref() {
            Some(Ok(token)) => token,
            Some(Err(_)) => {
                return Err(self
                    .lexer
                    .next()
                    .unwrap()
                    .unwrap_err()
                    .context("while parsing expression"))
            }
            None => {
                return Err(parser_unexpected_end_of_file(
                    (self.lexer.byte_offset, 0),
                    "an expression",
                ))
                .context("while parsing expression");
            }
        };

        let handler = self
            .lookup
            .expression_lookup
            .get(&token.kind)
            .ok_or(parser_expected_expression(token))
            .context("while parsing expression")?;

        let mut lhs = handler(self).context("while parsing expression")?;

        let mut next_token = self.lexer.peek();

        while let Some(token) = next_token {
            let token = match token {
                Ok(token) => token,
                Err(_) => {
                    return Err(self
                        .lexer
                        .next()
                        .unwrap()
                        .unwrap_err()
                        .context("while parsing expression"))
                }
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

            lhs = handler(self, lhs, token_binding_power)
                .context("while parsing left-hand side expression")?;

            next_token = self.lexer.peek();
        }

        Ok(lhs)
    }
}
