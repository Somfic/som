use super::{
    ast::{BinaryOperation, Expression, Statement, Type},
    expression,
    macros::{expect_expression, expect_token},
    statement, typing, Diagnostic, Parser,
};
use crate::scanner::lexeme::{TokenType, TokenValue};
use core::panic;
use std::collections::HashMap;

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

pub type TypeHandler = fn(&Parser, usize) -> Result<(Type, usize), Diagnostic>;
pub type LeftTypeHandler =
    fn(&Parser, usize, Type, &BindingPower) -> Result<(Type, usize), Diagnostic>;
pub type StatementHandler = fn(&Parser, usize) -> Result<(Statement, usize), Diagnostic>;
pub type ExpressionHandler = fn(&Parser, usize) -> Result<(Expression, usize), Diagnostic>;
pub type LeftExpressionHandler =
    fn(&Parser, usize, Expression, &BindingPower) -> Result<(Expression, usize), Diagnostic>;

pub struct Lookup {
    pub statement_lookup: HashMap<TokenType, StatementHandler>,
    pub expression_lookup: HashMap<TokenType, ExpressionHandler>,
    pub left_expression_lookup: HashMap<TokenType, LeftExpressionHandler>,
    pub type_lookup: HashMap<TokenType, TypeHandler>,
    pub left_type_lookup: HashMap<TokenType, LeftTypeHandler>,
    pub binding_power_lookup: HashMap<TokenType, BindingPower>,
}

impl Lookup {
    pub(crate) fn add_statement_handler(&mut self, token: TokenType, handler: StatementHandler) {
        if self.statement_lookup.contains_key(&token) {
            panic!("Token already has a statement handler");
        }

        self.statement_lookup.insert(token, handler);
    }

    pub(crate) fn add_expression_handler(&mut self, token: TokenType, handler: ExpressionHandler) {
        if self.expression_lookup.contains_key(&token) {
            panic!("Token already has an expression handler");
        }

        self.expression_lookup.insert(token, handler);
    }

    pub(crate) fn add_left_expression_handler(
        &mut self,
        token: TokenType,
        binding_power: BindingPower,
        handler: LeftExpressionHandler,
    ) {
        if self.binding_power_lookup.contains_key(&token) {
            panic!("Token already has a binding power");
        }

        self.left_expression_lookup.insert(token.clone(), handler);
        self.binding_power_lookup.insert(token, binding_power);
    }

    pub(crate) fn add_type_handler(&mut self, token: TokenType, handler: TypeHandler) {
        if self.type_lookup.contains_key(&token) {
            panic!("Token already has a type handler");
        }

        self.type_lookup.insert(token, handler);
    }

    #[allow(dead_code)]
    pub(crate) fn add_left_type_handler(&mut self, token: TokenType, handler: LeftTypeHandler) {
        if self.left_type_lookup.contains_key(&token) {
            panic!("Token already has a left type handler");
        }

        self.left_type_lookup.insert(token, handler);
    }
}

impl Default for Lookup {
    fn default() -> Self {
        let mut lookup = Lookup {
            statement_lookup: HashMap::new(),
            expression_lookup: HashMap::new(),
            left_expression_lookup: HashMap::new(),
            binding_power_lookup: HashMap::new(),
            type_lookup: HashMap::new(),
            left_type_lookup: HashMap::new(),
        };

        // Addative
        lookup.add_left_expression_handler(
            TokenType::Plus,
            BindingPower::Additive,
            |parser, cursor, lhs, _binding| {
                let (_, cursor) = expect_token!(parser, cursor, TokenType::Plus)?;
                let (rhs, cursor) = expect_expression!(parser, cursor, &BindingPower::Additive)?;
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
                let (_, cursor) = expect_token!(parser, cursor, TokenType::Minus)?;
                let (rhs, cursor) = expect_expression!(parser, cursor, &BindingPower::Additive)?;
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
                let (_, cursor) = expect_token!(parser, cursor, TokenType::Star)?;
                let (rhs, cursor) =
                    expect_expression!(parser, cursor, &BindingPower::Multiplicative)?;
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
                let (_, cursor) = expect_token!(parser, cursor, TokenType::Slash)?;
                let (rhs, cursor) =
                    expect_expression!(parser, cursor, &BindingPower::Multiplicative)?;
                Ok((
                    Expression::Binary(Box::new(lhs), BinaryOperation::Divide, Box::new(rhs)),
                    cursor,
                ))
            },
        );

        // Literals and symbols
        lookup.add_expression_handler(TokenType::Decimal, |parser, cursor| {
            let (decimal, cursor) = expect_token!(parser, cursor, TokenType::Decimal)?;

            if let TokenValue::Decimal(value) = decimal.value {
                return Ok((Expression::Number(value), cursor));
            }

            panic!("expect_token! should return a valid token and handle the error case");
        });

        lookup.add_expression_handler(TokenType::Integer, |parser, cursor| {
            let (integer, cursor) = expect_token!(parser, cursor, TokenType::Integer)?;

            if let TokenValue::Integer(value) = integer.value {
                return Ok((Expression::Number(value as f64), cursor));
            }

            panic!("expect_token! should return a valid token and handle the error case");
        });

        lookup.add_expression_handler(TokenType::String, |parser, cursor| {
            let (string, cursor) = expect_token!(parser, cursor, TokenType::String)?;

            if let TokenValue::String(string) = string.value.clone() {
                return Ok((Expression::String(string), cursor));
            }

            panic!("expect_token! should return a valid token and handle the error case");
        });

        lookup.add_expression_handler(TokenType::Identifier, |parser, cursor| {
            let (identifier, cursor) = expect_token!(parser, cursor, TokenType::Identifier)?;

            if let TokenValue::Identifier(identifier) = identifier.value.clone() {
                return Ok((Expression::Identifier(identifier), cursor));
            }

            panic!("expect_token! should return a valid token and handle the error case");
        });

        lookup.add_expression_handler(TokenType::ParenOpen, |parser, cursor| {
            let (_, cursor) = expect_token!(parser, cursor, TokenType::ParenOpen)?;
            let (expression, cursor) = expect_expression!(parser, cursor, &BindingPower::None)?;
            let (_, cursor) = expect_token!(parser, cursor, TokenType::ParenClose)?;

            Ok((Expression::Grouping(Box::new(expression)), cursor))
        });

        lookup.add_expression_handler(TokenType::Minus, expression::parse_unary);
        lookup.add_expression_handler(TokenType::Not, expression::parse_unary);
        lookup.add_statement_handler(TokenType::Var, statement::parse_declaration);
        lookup.add_left_expression_handler(
            TokenType::Equal,
            BindingPower::Assignment,
            expression::parse_assignment,
        );

        lookup.add_type_handler(TokenType::Identifier, typing::parse_symbol);
        lookup.add_type_handler(TokenType::SquareOpen, typing::parse_array);
        lookup.add_statement_handler(TokenType::Struct, statement::parse_struct);

        lookup
    }
}
