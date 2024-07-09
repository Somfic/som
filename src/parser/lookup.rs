use super::{
    ast::{BinaryOperation, Expression, Statement, Type},
    ParseResult, Parser,
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

pub type TypeHandler<'a> = fn(&mut Parser<'a>) -> ParseResult<'a, Type>;
pub type LeftTypeHandler<'a> = fn(&mut Parser<'a>, Type, &BindingPower) -> ParseResult<'a, Type>;
pub type StatementHandler<'a> = fn(&mut Parser<'a>) -> ParseResult<'a, Statement>;
pub type ExpressionHandler<'a> = fn(&mut Parser<'a>) -> ParseResult<'a, Expression>;
pub type LeftExpressionHandler<'a> =
    fn(&mut Parser<'a>, Expression, &BindingPower) -> ParseResult<'a, Expression>;

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
    ) -> &mut Self {
        if self.statement_lookup.contains_key(&token) {
            panic!("Token already has a statement handler");
        }

        self.statement_lookup.insert(token, handler);

        self
    }

    pub(crate) fn add_expression_handler(
        &mut self,
        token_type: TokenType,
        handler: ExpressionHandler<'a>,
    ) -> &mut Self {
        if self.expression_lookup.contains_key(&token_type) {
            panic!("Token already has an expression handler");
        }

        self.expression_lookup.insert(token_type, handler);

        self
    }

    pub(crate) fn add_left_expression_handler(
        &mut self,
        token_type: TokenType,
        binding_power: BindingPower,
        handler: LeftExpressionHandler<'a>,
    ) -> &mut Self {
        if self.binding_power_lookup.contains_key(&token_type) {
            panic!("Token already has a binding power");
        }

        self.left_expression_lookup
            .insert(token_type.clone(), handler);
        self.binding_power_lookup.insert(token_type, binding_power);

        self
    }

    pub(crate) fn add_type_handler(
        &mut self,
        token_type: TokenType,
        handler: TypeHandler<'a>,
    ) -> &mut Self {
        if self.type_lookup.contains_key(&token_type) {
            panic!("Token already has a type handler");
        }

        self.type_lookup.insert(token_type, handler);

        self
    }

    #[allow(dead_code)]
    pub(crate) fn add_left_type_handler(
        &mut self,
        token_type: TokenType,
        handler: LeftTypeHandler<'a>,
    ) {
        if self.left_type_lookup.contains_key(&token_type) {
            panic!("Token already has a left type handler");
        }

        self.left_type_lookup.insert(token_type, handler);
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

        super::expression::register(&mut lookup);

        lookup
    }
}
