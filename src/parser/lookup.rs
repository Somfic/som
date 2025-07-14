use crate::{expressions, prelude::*, statements, types};

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum BindingPower {
    None = 0,
    Comma = 1,
    Assignment = 2,
    Declaration = 3,
    Logical = 4,
    Relational = 5,
    Additive = 6,
    Multiplicative = 7,
    Unary = 8,
    Call = 9,
    Member = 10,
    Primary = 11,
}

pub type TypeHandler = fn(&mut Parser) -> Result<Type>;
pub type LeftTypeHandler = fn(&mut Parser, Type, BindingPower) -> Result<Type>;
pub type StatementHandler = fn(&mut Parser) -> Result<Statement>;
pub type ExpressionHandler = fn(&mut Parser) -> Result<Expression>;
pub type LeftExpressionHandler = fn(&mut Parser, Expression, BindingPower) -> Result<Expression>;

pub struct Lookup {
    pub statement_lookup: HashMap<TokenKind, StatementHandler>,
    pub expression_lookup: HashMap<TokenKind, ExpressionHandler>,
    pub left_expression_lookup: HashMap<TokenKind, LeftExpressionHandler>,
    pub type_lookup: HashMap<TokenKind, TypeHandler>,
    pub left_type_lookup: HashMap<TokenKind, LeftTypeHandler>,
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

    pub fn add_type_handler(mut self, token: TokenKind, handler: TypeHandler) -> Self {
        if self.type_lookup.contains_key(&token) {
            panic!("Token already has a type handler");
        }

        self.type_lookup.insert(token, handler);
        self
    }

    pub fn add_left_type_handler(mut self, token: TokenKind, handler: LeftTypeHandler) -> Self {
        if self.left_type_lookup.contains_key(&token) {
            panic!("Token already has a left type handler");
        }

        self.left_type_lookup.insert(token, handler);
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
            type_lookup: HashMap::new(),
            left_type_lookup: HashMap::new(),
        }
        .add_expression_handler(TokenKind::ParenOpen, crate::expressions::group::parse)
        .add_expression_handler(TokenKind::CurlyOpen, crate::expressions::block::parse)
        .add_expression_handler(TokenKind::Identifier, crate::expressions::identifier::parse)
        .add_expression_handler(TokenKind::Function, crate::expressions::function::parse)
        .add_expression_handler(
            TokenKind::I32,
            crate::expressions::primary::integer::parse_i32,
        )
        .add_expression_handler(
            TokenKind::I64,
            crate::expressions::primary::integer::parse_i64,
        )
        .add_expression_handler(
            TokenKind::Boolean,
            crate::expressions::primary::boolean::parse,
        )
        .add_expression_handler(
            TokenKind::String,
            crate::expressions::primary::string::parse,
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
        .add_left_expression_handler(
            TokenKind::ParenOpen,
            BindingPower::Call,
            crate::expressions::call::parse,
        )
        .add_statement_handler(
            TokenKind::Let,
            crate::statements::variable_declaration::parse,
        )
        .add_type_handler(TokenKind::I32Type, crate::types::integer::parse_i32)
        .add_type_handler(TokenKind::I64Type, crate::types::integer::parse_i64)
        .add_type_handler(TokenKind::BooleanType, crate::types::boolean::parse)
        .add_type_handler(TokenKind::StringType, crate::types::string::parse)
        .add_left_expression_handler(
            TokenKind::If,
            BindingPower::Logical,
            crate::expressions::conditional::parse,
        )
        .add_statement_handler(TokenKind::Extern, statements::extern_declaration::parse)
        .add_statement_handler(TokenKind::Type, statements::type_declaration::parse)
        .add_type_handler(TokenKind::CurlyOpen, types::struct_::parse)
        .add_left_expression_handler(
            TokenKind::CurlyOpen,
            BindingPower::Declaration,
            crate::expressions::struct_constructor::parse,
        )
        .add_left_expression_handler(
            TokenKind::Dot,
            BindingPower::Member,
            crate::expressions::field_access::parse,
        )
        .add_expression_handler(TokenKind::Minus, expressions::unary::negative::parse)
    }
}
