use std::collections::HashMap;

use super::{expression, statement, typing, Parser};
use crate::{
    ast::{Expression, Statement, Typing},
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

pub type TypingHandler<'ast> = fn(&mut Parser<'ast>) -> ParserResult<Typing<'ast>>;
pub type LeftTypingHandler<'ast> =
    fn(&mut Parser<'ast>, Typing, BindingPower) -> ParserResult<Typing<'ast>>;
pub type StatementHandler<'ast> = fn(&mut Parser<'ast>) -> ParserResult<Statement<'ast>>;
pub type ExpressionHandler<'ast> = fn(&mut Parser<'ast>) -> ParserResult<Expression<'ast>>;
pub type LeftExpressionHandler<'ast> =
    fn(&mut Parser<'ast>, Expression<'ast>, BindingPower) -> ParserResult<Expression<'ast>>;

pub struct Lookup<'ast> {
    pub statement_lookup: HashMap<TokenKind, StatementHandler<'ast>>,
    pub expression_lookup: HashMap<TokenKind, ExpressionHandler<'ast>>,
    pub left_expression_lookup: HashMap<TokenKind, LeftExpressionHandler<'ast>>,
    pub typing_lookup: HashMap<TokenKind, TypingHandler<'ast>>,
    pub left_typing_lookup: HashMap<TokenKind, LeftTypingHandler<'ast>>,
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

    pub(crate) fn add_typing_handler(
        mut self,
        token: TokenKind,
        handler: TypingHandler<'ast>,
    ) -> Self {
        if self.typing_lookup.contains_key(&token) {
            panic!("Token already has a type handler");
        }

        self.typing_lookup.insert(token, handler);
        self
    }

    pub(crate) fn add_left_typing_handler(
        mut self,
        token: TokenKind,
        handler: LeftTypingHandler<'ast>,
    ) -> Self {
        if self.left_typing_lookup.contains_key(&token) {
            panic!("Token already has a left type handler");
        }

        self.left_typing_lookup.insert(token, handler);
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
            typing_lookup: HashMap::new(),
            left_typing_lookup: HashMap::new(),
        }
        .add_expression_handler(TokenKind::Integer, expression::parse_integer)
        .add_expression_handler(TokenKind::Decimal, expression::parse_decimal)
        .add_expression_handler(TokenKind::ParenOpen, expression::parse_group)
        .add_expression_handler(TokenKind::Minus, expression::parse_unary_negative)
        .add_expression_handler(TokenKind::Boolean, expression::parse_boolean)
        .add_expression_handler(TokenKind::CurlyOpen, expression::parse_block)
        .add_expression_handler(TokenKind::Identifier, expression::parse_identifier)
        .add_left_expression_handler(
            TokenKind::If,
            BindingPower::Logical,
            expression::parse_conditional,
        )
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
        .add_statement_handler(TokenKind::Let, statement::parse_let)
        .add_typing_handler(TokenKind::Identifier, typing::parse_symbol)
        .add_typing_handler(TokenKind::IntegerType, typing::parse_integer)
    }
}
