use std::collections::HashMap;

use super::{
    expression, macros::expect_token, BinaryOperation, Diagnostic, Expression, Parser, Statement,
};
use crate::scanner::lexeme::{Lexeme, TokenType, TokenValue};

#[derive(Debug, PartialEq, PartialOrd)]
pub enum BindingPower {
    None = 0,
    Comma = 1,
    Assignment = 2,
    Logical = 3,
    Relational = 4,
    Additive = 5,
    Multiplicative = 6,
    Unary = 7,
    Call = 8,
    Member = 9,
    Primary = 10,
}

pub type StatementHandler = fn(&Parser, usize) -> Result<(Statement, usize), Diagnostic>;
pub type ExpressionHandler = fn(&Parser, usize) -> Result<(Expression, usize), Diagnostic>;
pub type LeftExpressionHandler =
    fn(&Parser, usize, Expression, &BindingPower) -> Result<(Expression, usize), Diagnostic>;

pub struct Lookup {
    pub statement_lookup: HashMap<TokenType, StatementHandler>,
    pub expression_lookup: HashMap<TokenType, ExpressionHandler>,
    pub left_expression_lookup: HashMap<TokenType, LeftExpressionHandler>,
    pub binding_power_lookup: HashMap<TokenType, BindingPower>,
}

impl Lookup {
    pub(crate) fn add_statement_handler(&mut self, token: TokenType, handler: StatementHandler) {
        self.statement_lookup.insert(token, handler);
    }

    pub(crate) fn add_expression_handler(&mut self, token: TokenType, handler: ExpressionHandler) {
        self.expression_lookup.insert(token, handler);
    }

    pub(crate) fn add_left_expression_handler(
        &mut self,
        token: TokenType,
        binding_power: BindingPower,
        handler: LeftExpressionHandler,
    ) {
        self.left_expression_lookup.insert(token.clone(), handler);
        self.binding_power_lookup.insert(token, binding_power);
    }
}

impl Default for Lookup {
    fn default() -> Self {
        let mut lookup = Lookup {
            statement_lookup: HashMap::new(),
            expression_lookup: HashMap::new(),
            left_expression_lookup: HashMap::new(),
            binding_power_lookup: HashMap::new(),
        };

        // Addative
        lookup.add_left_expression_handler(
            TokenType::Plus,
            BindingPower::Additive,
            |parser, cursor, lhs, _binding| {
                let (rhs, cursor) =
                    super::expression::parse(parser, cursor + 1, &BindingPower::Additive)?;
                Ok((
                    Expression::Binary(Box::new(lhs), BinaryOperation::Plus, Box::new(rhs)),
                    cursor,
                ))
            },
        );

        lookup.add_left_expression_handler(
            TokenType::Minus,
            BindingPower::Additive,
            |parser, cursor, lhs, _binding| {
                let (rhs, cursor) =
                    super::expression::parse(parser, cursor + 1, &BindingPower::Additive)?;
                Ok((
                    Expression::Binary(Box::new(lhs), BinaryOperation::Minus, Box::new(rhs)),
                    cursor,
                ))
            },
        );

        // Multiplicative
        lookup.add_left_expression_handler(
            TokenType::Star,
            BindingPower::Multiplicative,
            |parser, cursor, lhs, _binding| {
                let (rhs, cursor) =
                    super::expression::parse(parser, cursor + 1, &BindingPower::Multiplicative)?;
                Ok((
                    Expression::Binary(Box::new(lhs), BinaryOperation::Times, Box::new(rhs)),
                    cursor,
                ))
            },
        );

        lookup.add_left_expression_handler(
            TokenType::Slash,
            BindingPower::Multiplicative,
            |parser, cursor, lhs, _binding| {
                let (rhs, cursor) =
                    super::expression::parse(parser, cursor + 1, &BindingPower::Multiplicative)?;
                Ok((
                    Expression::Binary(Box::new(lhs), BinaryOperation::Divide, Box::new(rhs)),
                    cursor,
                ))
            },
        );

        // Literals and symbols
        lookup.add_expression_handler(TokenType::Decimal, |parser, cursor| {
            let (decimal, cursor) = expect_token!(parser, cursor, TokenType::Decimal)?;
            if let Lexeme::Valid(decimal) = decimal {
                if let TokenValue::Decimal(value) = decimal.value {
                    return Ok((Expression::Number(value), cursor));
                }
                panic!("Token with decimal type does not have a decimal value");
            }

            Err(Diagnostic::error(decimal.range(), "Expected a decimal"))
        });

        lookup.add_expression_handler(TokenType::Integer, |parser, cursor| {
            let (integer, cursor) = expect_token!(parser, cursor, TokenType::Integer)?;
            if let Lexeme::Valid(integer) = integer {
                if let TokenValue::Integer(value) = integer.value {
                    return Ok((Expression::Number(value as f64), cursor));
                }
                panic!("Token with integer type does not have an integer value");
            }

            Err(Diagnostic::error(integer.range(), "Expected an integer"))
        });

        lookup.add_expression_handler(TokenType::String, |parser, cursor| {
            let (string, cursor) = expect_token!(parser, cursor, TokenType::String)?;
            if let Lexeme::Valid(string) = string {
                if let TokenValue::String(string) = string.value.clone() {
                    return Ok((Expression::String(string), cursor));
                }
                panic!("Token with string type does not have a string value");
            }

            Err(Diagnostic::error(string.range(), "Expected a string"))
        });

        lookup.add_expression_handler(TokenType::Identifier, |parser, cursor| {
            let (identifier, cursor) = expect_token!(parser, cursor, TokenType::Identifier)?;
            if let Lexeme::Valid(identifier) = identifier {
                if let TokenValue::Identifier(identifier) = identifier.value.clone() {
                    return Ok((Expression::Identifier(identifier), cursor));
                }
                panic!("Token with identifier type does not have an identifier value");
            }

            Err(Diagnostic::error(
                identifier.range(),
                "Expected an identifier",
            ))
        });

        lookup.add_statement_handler(TokenType::Semicolon, |parser, cursor| {
            let (expression, cursor) = expression::parse(parser, cursor, &BindingPower::Primary)?;
            let (_, cursor) = expect_token!(parser, cursor, TokenType::Semicolon)?;
            Ok((Statement::Expression(expression), cursor))
        });

        lookup.add_expression_handler(TokenType::ParenOpen, |parser, cursor| {
            let (_, cursor) = expect_token!(parser, cursor, TokenType::ParenOpen)?;
            let (expression, cursor) = expression::parse(parser, cursor, &BindingPower::None)?;
            let (_, cursor) = expect_token!(parser, cursor, TokenType::ParenClose)?;

            Ok((Expression::Grouping(Box::new(expression)), cursor))
        });

        lookup
    }
}
