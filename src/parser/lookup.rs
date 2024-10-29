use crate::lexer::{TokenKind, TokenValue};
use miette::Result;
use std::collections::HashMap;

use super::{
    ast::{BinaryOperator, Expression, Primitive, Statement},
    expression, Parser,
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

// pub type TypeHandler<'de> = fn(&mut Parser<'de>) -> Result<(Type, usize), Error<'de>>;
// pub type LeftTypeHandler<'de> =
//     fn(&'de Parser<'de>, usize, Type, &BindingPower) -> Result<(Type, usize), Error<'de>>;
pub type StatementHandler<'de> = fn(&mut Parser<'de>) -> Result<Statement<'de>>;
pub type ExpressionHandler<'de> = fn(&mut Parser<'de>) -> Result<Expression<'de>>;
pub type LeftExpressionHandler<'de> =
    fn(&mut Parser<'de>, Expression<'de>, BindingPower) -> Result<Expression<'de>>;

pub struct Lookup<'de> {
    pub statement_lookup: HashMap<TokenKind, StatementHandler<'de>>,
    pub expression_lookup: HashMap<TokenKind, ExpressionHandler<'de>>,
    pub left_expression_lookup: HashMap<TokenKind, LeftExpressionHandler<'de>>,
    // pub type_lookup: HashMap<TokenKind, TypeHandler<'de>>,
    // pub left_type_lookup: HashMap<TokenKind, LeftTypeHandler<'de>>,
    pub binding_power_lookup: HashMap<TokenKind, BindingPower>,
}

impl<'de> Lookup<'de> {
    pub(crate) fn add_statement_handler(
        mut self,
        token: TokenKind,
        handler: StatementHandler<'de>,
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
        handler: ExpressionHandler<'de>,
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
        handler: LeftExpressionHandler<'de>,
    ) -> Self {
        if self.binding_power_lookup.contains_key(&token) {
            panic!("Token already has a binding power");
        }

        self.left_expression_lookup.insert(token.clone(), handler);
        self.binding_power_lookup.insert(token, binding_power);
        self
    }

    // pub(crate) fn add_type_handler(&mut self, token: TokenType, handler: TypeHandler<'de>) {
    //     if self.type_lookup.contains_key(&token) {
    //         panic!("Token already has a type handler");
    //     }

    //     self.type_lookup.insert(token, handler);
    // }

    // #[allow(dead_code)]
    // pub(crate) fn add_left_type_handler(&mut self, token: TokenType, handler: LeftTypeHandler<'de>) {
    //     if self.left_type_lookup.contains_key(&token) {
    //         panic!("Token already has a left type handler");
    //     }

    //     self.left_type_lookup.insert(token, handler);
    // }
}

impl<'de> Default for Lookup<'de> {
    fn default() -> Self {
        Lookup {
            statement_lookup: HashMap::new(),
            expression_lookup: HashMap::new(),
            left_expression_lookup: HashMap::new(),
            binding_power_lookup: HashMap::new(),
            // type_lookup: HashMap::new(),
            // left_type_lookup: HashMap::new(),
        }
        .add_expression_handler(TokenKind::Integer, expression::primitive::integer)
        .add_expression_handler(TokenKind::Decimal, expression::primitive::decimal)
        .add_expression_handler(TokenKind::ParenOpen, group)
        .add_expression_handler(TokenKind::If, if_)
        .add_left_expression_handler(
            TokenKind::Plus,
            BindingPower::Additive,
            expression::binary::addition,
        )
        .add_left_expression_handler(
            TokenKind::Minus,
            BindingPower::Additive,
            expression::binary::subtraction,
        )
        .add_left_expression_handler(
            TokenKind::Star,
            BindingPower::Multiplicative,
            expression::binary::multiplication,
        )
        .add_left_expression_handler(
            TokenKind::Slash,
            BindingPower::Multiplicative,
            expression::binary::division,
        )
    }
}

fn if_<'de>(parser: &mut Parser) -> Result<Expression<'de>> {
    parser.lexer.expect(TokenKind::If, "expected an if")?;
    let condition = expression::parse(parser, BindingPower::None)?;

    todo!()
}

fn group<'de>(parser: &mut Parser<'de>) -> Result<Expression<'de>> {
    parser
        .lexer
        .expect(TokenKind::ParenOpen, "expected a left parenthesis")?;
    let expression = expression::parse(parser, BindingPower::None)?;
    parser
        .lexer
        .expect(TokenKind::ParenClose, "expected a right parenthesis")?;

    Ok(Expression::Group(Box::new(expression)))
}
