use crate::prelude::*;
use std::cell::RefCell;

pub mod lookup;

pub struct Parser<'source> {
    pub errors: RefCell<Vec<Error>>,
    pub lexer: Lexer<'source>,
    lookup: Lookup,
}

impl<'source> Parser<'source> {
    pub fn new(lexer: Lexer<'source>) -> Self {
        Self {
            errors: RefCell::new(Vec::new()),
            lexer,
            lookup: Lookup::default(),
        }
    }

    pub fn parse(&mut self) -> Results<Statement> {
        match self.parse_statement(true) {
            Ok(statement) => Ok(statement),
            Err(e) => {
                self.errors.borrow_mut().push(e);
                return Err(self.errors.borrow().clone());
            }
        }
    }

    pub fn expect(&mut self, expected: TokenKind, help: impl Into<String>) -> Result<Token> {
        match self.lexer.next() {
            Some(Ok(token)) if expected == token.kind => Ok(token),
            Some(Ok(token)) => Err(parser_unexpected_token(help, &token, &expected)),
            Some(Err(e)) => Err(e),
            None => Err(parser_unexpected_end_of_file(
                (self.lexer.byte_offset - 1, 0),
                format!("{}", expected),
            )),
        }
    }

    pub fn peek(&mut self) -> Option<&Result<Token>> {
        self.lexer.peek()
    }

    pub fn next(&mut self) -> Option<Result<Token>> {
        self.lexer.next()
    }

    pub fn parse_statement(&mut self, require_semicolon: bool) -> Result<Statement> {
        let token = match self.lexer.peek().as_ref() {
            Some(Ok(token)) => token,
            Some(Err(_)) => return Err(self.lexer.next().unwrap().unwrap_err()),
            None => {
                return Err(parser_unexpected_end_of_file(
                    (self.lexer.byte_offset, 0),
                    "a statement",
                ));
            }
        };

        let statement = match self.lookup.statement_lookup.get(&token.kind) {
            Some(handler) => handler(self)?,
            None => {
                let expression = self.parse_expression(BindingPower::None)?;

                Statement {
                    value: StatementValue::Expression(expression.clone()),
                    span: expression.span,
                }
            }
        };

        if require_semicolon {
            self.expect(TokenKind::Semicolon, "expected a closing semicolon")?;
        }

        Ok(statement)
    }

    pub fn parse_expression(&mut self, bp: BindingPower) -> Result<Expression> {
        let token = match self.lexer.peek().as_ref() {
            Some(Ok(token)) => token,
            Some(Err(_)) => return Err(self.lexer.next().unwrap().unwrap_err()),
            None => {
                return Err(parser_unexpected_end_of_file(
                    (self.lexer.byte_offset, 0),
                    "an expression",
                ));
            }
        };

        let handler = self
            .lookup
            .expression_lookup
            .get(&token.kind)
            .ok_or(parser_expected_expression(token))?;

        let mut lhs = handler(self)?;

        let mut next_token = self.lexer.peek();

        while let Some(token) = next_token {
            let token = match token {
                Ok(token) => token,
                Err(_) => return Err(self.lexer.next().unwrap().unwrap_err()),
            };

            let token_binding_power = {
                let binding_power_lookup = self.lookup.binding_power_lookup.clone();
                *binding_power_lookup
                    .get(&token.kind)
                    .unwrap_or(&BindingPower::None)
            };

            if bp >= token_binding_power {
                break;
            }

            let handler = match self.lookup.left_expression_lookup.get(&token.kind) {
                Some(handler) => handler,
                None => break,
            };

            lhs = handler(self, lhs, token_binding_power)?;

            next_token = self.lexer.peek();
        }

        Ok(lhs)
    }

    pub fn parse_type(&mut self, bp: BindingPower) -> Result<Type> {
        let token = match self.lexer.peek().as_ref() {
            Some(Ok(token)) => token,
            Some(Err(_)) => return Err(self.lexer.next().unwrap().unwrap_err()),
            None => {
                return Err(parser_unexpected_end_of_file(
                    (self.lexer.byte_offset, 0),
                    "a type",
                ));
            }
        };

        let handler = self
            .lookup
            .type_lookup
            .get(&token.kind)
            .ok_or(parser_expected_type(token))?;

        let mut lhs = handler(self)?;

        let mut next_token = self.lexer.peek();

        while let Some(token) = next_token {
            let token = match token {
                Ok(token) => token,
                Err(_) => return Err(self.lexer.next().unwrap().unwrap_err()),
            };

            let token_binding_power = {
                let binding_power_lookup = self.lookup.binding_power_lookup.clone();
                *binding_power_lookup
                    .get(&token.kind)
                    .unwrap_or(&BindingPower::None)
            };

            if bp >= token_binding_power {
                break;
            }

            let handler = match self.lookup.left_type_lookup.get(&token.kind) {
                Some(handler) => handler,
                None => break,
            };

            lhs = handler(self, lhs, token_binding_power)?;

            next_token = self.lexer.peek();
        }

        Ok(lhs)
    }

    pub fn expect_identifier(&mut self) -> Result<Identifier> {
        let token = self.expect(TokenKind::Identifier, "expected an identifier")?;

        let value = match token.value {
            TokenValue::Identifier(value) => value,
            _ => unreachable!(),
        };

        Ok(value)
    }

    pub fn expect_list<T>(
        &mut self,
        open: TokenKind,
        parse: fn(&mut Self) -> Result<T>,
        divider: TokenKind,
        close: TokenKind,
    ) -> Result<(Vec<T>, Span)> {
        let start = self.expect(open, "expected a list")?;
        let mut list = vec![];

        loop {
            if self
                .peek()
                .is_some_and(|token| token.as_ref().is_ok_and(|token| token.kind == close))
            {
                break;
            }

            if !list.is_empty() {
                self.expect(divider.clone(), "expected a comma between arguments")?;
            }

            let value = parse(self)?;

            list.push(value);
        }

        let end = self.expect(close, "expected a list")?;

        let span = start.span + end.span;

        Ok((list, span))
    }
}
