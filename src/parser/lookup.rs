use super::{
    ast::{BinaryOperation, Expression, Statement, Type},
    expression,
    macros::{expect_expression, expect_tokens},
    statement, typing, Parser,
};
use crate::{
    diagnostic::Error,
    parser::macros::expect_token_value,
    scanner::lexeme::{TokenType, TokenValue},
};
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

pub type TypeHandler<'a> = fn(&'a Parser<'a>, usize) -> Result<(Type, usize), Error<'a>>;
pub type LeftTypeHandler<'a> =
    fn(&'a Parser<'a>, usize, Type, &BindingPower) -> Result<(Type, usize), Error<'a>>;
pub type StatementHandler<'a> = fn(&'a Parser<'a>, usize) -> Result<(Statement, usize), Error<'a>>;
pub type ExpressionHandler<'a> =
    fn(&'a Parser<'a>, usize) -> Result<(Expression, usize), Error<'a>>;
pub type LeftExpressionHandler<'a> =
    fn(&'a Parser<'a>, usize, Expression, &BindingPower) -> Result<(Expression, usize), Error<'a>>;

pub struct Lookup<'a> {
    pub statement_lookup: HashMap<TokenType, StatementHandler<'a>>,
    pub expression_lookup: HashMap<TokenType, ExpressionHandler<'a>>,
    pub left_expression_lookup: HashMap<TokenType, LeftExpressionHandler<'a>>,
    pub type_lookup: HashMap<TokenType, TypeHandler<'a>>,
    pub left_type_lookup: HashMap<TokenType, LeftTypeHandler<'a>>,
    pub binding_power_lookup: HashMap<TokenType, BindingPower>,
}

impl<'a> Lookup<'a> {
    pub(crate) fn add_statement_handler(
        &mut self,
        token: TokenType,
        handler: StatementHandler<'a>,
    ) {
        if self.statement_lookup.contains_key(&token) {
            panic!("Token already has a statement handler");
        }

        self.statement_lookup.insert(token, handler);
    }

    pub(crate) fn add_expression_handler(
        &mut self,
        token: TokenType,
        handler: ExpressionHandler<'a>,
    ) {
        if self.expression_lookup.contains_key(&token) {
            panic!("Token already has an expression handler");
        }

        self.expression_lookup.insert(token, handler);
    }

    pub(crate) fn add_left_expression_handler(
        &mut self,
        token: TokenType,
        binding_power: BindingPower,
        handler: LeftExpressionHandler<'a>,
    ) {
        if self.binding_power_lookup.contains_key(&token) {
            panic!("Token already has a binding power");
        }

        self.left_expression_lookup.insert(token.clone(), handler);
        self.binding_power_lookup.insert(token, binding_power);
    }

    pub(crate) fn add_type_handler(&mut self, token: TokenType, handler: TypeHandler<'a>) {
        if self.type_lookup.contains_key(&token) {
            panic!("Token already has a type handler");
        }

        self.type_lookup.insert(token, handler);
    }

    #[allow(dead_code)]
    pub(crate) fn add_left_type_handler(&mut self, token: TokenType, handler: LeftTypeHandler<'a>) {
        if self.left_type_lookup.contains_key(&token) {
            panic!("Token already has a left type handler");
        }

        self.left_type_lookup.insert(token, handler);
    }
}

impl<'a> Default for Lookup<'a> {
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
                let (_, cursor) = expect_tokens!(parser, cursor, TokenType::Plus)?;
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
                let (_, cursor) = expect_tokens!(parser, cursor, TokenType::Minus)?;
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
                let (_, cursor) = expect_tokens!(parser, cursor, TokenType::Star)?;
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
                let (_, cursor) = expect_tokens!(parser, cursor, TokenType::Slash)?;
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
            let (decimal, cursor) = expect_tokens!(parser, cursor, TokenType::Decimal)?;
            let decimal = expect_token_value!(decimal[0], TokenValue::Decimal);
            Ok((Expression::Number(decimal), cursor))
        });

        lookup.add_expression_handler(TokenType::Integer, |parser, cursor| {
            let (integer, cursor) = expect_tokens!(parser, cursor, TokenType::Integer)?;
            let integer = expect_token_value!(integer[0], TokenValue::Integer);
            Ok((Expression::Number(integer as f64), cursor))
        });

        lookup.add_expression_handler(TokenType::String, |parser, cursor| {
            let (string, cursor) = expect_tokens!(parser, cursor, TokenType::String)?;
            let string = expect_token_value!(string[0], TokenValue::String);
            Ok((Expression::String(string), cursor))
        });

        lookup.add_expression_handler(TokenType::Identifier, |parser, cursor| {
            let (identifier, cursor) = expect_tokens!(parser, cursor, TokenType::Identifier)?;
            let identifier = expect_token_value!(identifier[0], TokenValue::Identifier);
            Ok((Expression::Identifier(identifier), cursor))
        });

        lookup.add_expression_handler(TokenType::ParenOpen, |parser, cursor| {
            let (_, cursor) = expect_tokens!(parser, cursor, TokenType::ParenOpen)?;
            let (expression, cursor) = expect_expression!(parser, cursor, &BindingPower::None)?;
            let (_, cursor) = expect_tokens!(parser, cursor, TokenType::ParenClose)?;
            Ok((Expression::Grouping(Box::new(expression)), cursor))
        });

        lookup.add_expression_handler(TokenType::Minus, expression::parse_unary);
        lookup.add_expression_handler(TokenType::Not, expression::parse_unary);
        lookup.add_statement_handler(TokenType::Let, statement::parse_declaration);
        lookup.add_left_expression_handler(
            TokenType::Equal,
            BindingPower::Assignment,
            expression::parse_assignment,
        );

        lookup.add_left_expression_handler(
            TokenType::CurlyOpen,
            BindingPower::Primary,
            expression::parse_struct_initializer,
        );
        lookup.add_type_handler(TokenType::Identifier, typing::parse_symbol);
        lookup.add_type_handler(TokenType::SquareOpen, typing::parse_array);
        lookup.add_type_handler(TokenType::CurlyOpen, typing::parse_tuple);

        lookup.add_statement_handler(TokenType::Struct, statement::parse_struct);
        lookup.add_statement_handler(TokenType::Enum, statement::parse_enum);

        lookup
    }
}
