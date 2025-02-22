use std::collections::HashMap;

use super::{expression, Parser};
use crate::{
    ast::{Expression, Statement, Type},
    prelude::*,
    tokenizer::TokenKind,
};

#[derive(Debug, Clone, PartialEq, PartialOrd)]
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

pub type TypeHandler<'ast> = fn(&mut Parser<'ast>) -> ParserResult<Type<'ast>>;
pub type LeftTypeHandler<'ast> =
    fn(&mut Parser<'ast>, Type, BindingPower) -> ParserResult<Type<'ast>>;
pub type StatementHandler<'ast> = fn(&mut Parser<'ast>) -> ParserResult<Statement<'ast>>;
pub type ExpressionHandler<'ast> = fn(&mut Parser<'ast>) -> ParserResult<Expression<'ast>>;
pub type LeftExpressionHandler<'ast> =
    fn(&mut Parser<'ast>, Expression<'ast>, BindingPower) -> ParserResult<Expression<'ast>>;

pub struct Lookup<'ast> {
    pub statement_lookup: HashMap<TokenKind, StatementHandler<'ast>>,
    pub expression_lookup: HashMap<TokenKind, ExpressionHandler<'ast>>,
    pub left_expression_lookup: HashMap<TokenKind, LeftExpressionHandler<'ast>>,
    pub type_lookup: HashMap<TokenKind, TypeHandler<'ast>>,
    pub left_type_lookup: HashMap<TokenKind, LeftTypeHandler<'ast>>,
    pub binding_power_lookup: HashMap<TokenKind, BindingPower>,
}

impl<'ast> Lookup<'ast> {
    pub(crate) fn add_statement_handler(
        mut self,
        token: TokenKind,
        handler: StatementHandler<'ast>,
    ) -> Self {
        if self.statement_lookup.contains_key(&token) {
            panic!("Token already has a statement handler");
        }

        self.statement_lookup.insert(token, handler);
        self
    }

    pub(crate) fn add_expression_handler(
        mut self,
        token: TokenKind,
        handler: ExpressionHandler<'ast>,
    ) -> Self {
        if self.expression_lookup.contains_key(&token) {
            panic!("Token already has an expression handler");
        }

        self.expression_lookup.insert(token, handler);
        self
    }

    pub(crate) fn add_left_expression_handler(
        mut self,
        token: TokenKind,
        binding_power: BindingPower,
        handler: LeftExpressionHandler<'ast>,
    ) -> Self {
        if self.binding_power_lookup.contains_key(&token) {
            panic!("Token already has a binding power");
        }

        self.left_expression_lookup.insert(token.clone(), handler);
        self.binding_power_lookup.insert(token, binding_power);
        self
    }

    pub(crate) fn add_type_handler(mut self, token: TokenKind, handler: TypeHandler<'ast>) -> Self {
        if self.type_lookup.contains_key(&token) {
            panic!("Token already has a type handler");
        }

        self.type_lookup.insert(token, handler);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn add_left_type_handler(
        mut self,
        token: TokenKind,
        handler: LeftTypeHandler<'ast>,
    ) -> Self {
        if self.left_type_lookup.contains_key(&token) {
            panic!("Token already has a left type handler");
        }

        self.left_type_lookup.insert(token, handler);
        self
    }
}

impl Default for Lookup<'_> {
    fn default() -> Self {
        Lookup {
            statement_lookup: HashMap::new(),
            expression_lookup: HashMap::new(),
            left_expression_lookup: HashMap::new(),
            binding_power_lookup: HashMap::new(),
            type_lookup: HashMap::new(),
            left_type_lookup: HashMap::new(),
        }
        .add_expression_handler(TokenKind::Integer, expression::parse_integer)
        .add_expression_handler(TokenKind::Decimal, expression::parse_decimal)
        .add_expression_handler(TokenKind::ParenOpen, expression::parse_group)
        .add_left_expression_handler(
            TokenKind::Plus,
            BindingPower::Additive,
            expression::parse_binary_plus,
        )
        .add_left_expression_handler(
            TokenKind::Minus,
            BindingPower::Additive,
            expression::parse_binary_subtract,
        )
        .add_left_expression_handler(
            TokenKind::Star,
            BindingPower::Multiplicative,
            expression::parse_binary_multiply,
        )
        .add_left_expression_handler(
            TokenKind::Slash,
            BindingPower::Multiplicative,
            expression::parse_binary_divide,
        )
    }
}
