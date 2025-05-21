use crate::prelude::*;

use std::collections::HashMap;

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

pub type TypingHandler = fn(&mut Parser) -> Result<Type>;
pub type LeftTypingHandler = fn(&mut Parser, Type, BindingPower) -> Result<Type>;
pub type StatementHandler = fn(&mut Parser) -> Result<Statement>;
pub type ExpressionHandler = fn(&mut Parser) -> Result<Expression>;
pub type LeftExpressionHandler = fn(&mut Parser, Expression, BindingPower) -> Result<Expression>;

pub struct Lookup {
    pub statement_lookup: HashMap<TokenKind, StatementHandler>,
    pub expression_lookup: HashMap<TokenKind, ExpressionHandler>,
    pub left_expression_lookup: HashMap<TokenKind, LeftExpressionHandler>,
    pub typing_lookup: HashMap<TokenKind, TypingHandler>,
    pub left_typing_lookup: HashMap<TokenKind, LeftTypingHandler>,
    pub binding_power_lookup: HashMap<TokenKind, BindingPower>,
}

impl Lookup {
    pub fn add_statement_handler(mut self, token: TokenKind, handler: StatementHandler) -> Self {
        if self.statement_lookup.contains_key(&token) {
            panic!("Token already has a statement handler");
        }

        self.statement_lookup.insert(token, handler);
        self
    }

    pub fn add_expression_handler(mut self, token: TokenKind, handler: ExpressionHandler) -> Self {
        if self.expression_lookup.contains_key(&token) {
            panic!("Token already has an expression handler");
        }

        self.expression_lookup.insert(token, handler);
        self
    }

    pub fn add_left_expression_handler(
        mut self,
        token: TokenKind,
        binding_power: BindingPower,
        handler: LeftExpressionHandler,
    ) -> Self {
        if self.binding_power_lookup.contains_key(&token) {
            panic!("Token already has a binding power");
        }

        self.left_expression_lookup.insert(token.clone(), handler);
        self.binding_power_lookup.insert(token, binding_power);
        self
    }

    pub fn add_typing_handler(mut self, token: TokenKind, handler: TypingHandler) -> Self {
        if self.typing_lookup.contains_key(&token) {
            panic!("Token already has a type handler");
        }

        self.typing_lookup.insert(token, handler);
        self
    }

    pub fn add_left_typing_handler(mut self, token: TokenKind, handler: LeftTypingHandler) -> Self {
        if self.left_typing_lookup.contains_key(&token) {
            panic!("Token already has a left type handler");
        }

        self.left_typing_lookup.insert(token, handler);
        self
    }
}

impl Default for Lookup {
    fn default() -> Self {
        Lookup {
            statement_lookup: HashMap::new(),
            expression_lookup: HashMap::new(),
            left_expression_lookup: HashMap::new(),
            binding_power_lookup: HashMap::new(),
            typing_lookup: HashMap::new(),
            left_typing_lookup: HashMap::new(),
        }
        .add_expression_handler(TokenKind::ParenOpen, crate::expressions::group::parse)
        .add_expression_handler(
            TokenKind::Integer,
            crate::expressions::primary::integer::parse,
        )
        .add_expression_handler(
            TokenKind::Boolean,
            crate::expressions::primary::boolean::parse,
        )
        .add_left_expression_handler(
            TokenKind::Plus,
            BindingPower::Additive,
            crate::expressions::binary::add::parse,
        )
        .add_left_expression_handler(
            TokenKind::Minus,
            BindingPower::Additive,
            crate::expressions::binary::subtract::parse,
        )
        .add_left_expression_handler(
            TokenKind::Star,
            BindingPower::Multiplicative,
            crate::expressions::binary::multiply::parse,
        )
        .add_left_expression_handler(
            TokenKind::Slash,
            BindingPower::Multiplicative,
            crate::expressions::binary::divide::parse,
        )
    }
}
