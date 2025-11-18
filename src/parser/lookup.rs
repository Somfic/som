use crate::{
    ast::{Expression, Statement, Type},
    lexer::TokenKind,
    parser::{expression, Parse, Parser, Untyped},
    Result,
};
use std::{collections::HashMap, rc::Rc};

pub type ExpressionParser = Rc<dyn Fn(&mut Parser) -> Result<Expression<Untyped>>>;
pub type LefthandExpressionParser =
    Rc<dyn Fn(&mut Parser, Expression<Untyped>) -> Result<Expression<Untyped>>>;
pub type StatementParser = Rc<dyn Fn(&mut Parser) -> Result<Statement<Untyped>>>;
pub type TypeParser = Rc<dyn Fn(&mut Parser) -> Result<Type>>;

pub struct Lookup {
    pub statement_lookup: HashMap<TokenKind, StatementParser>,
    pub expression_lookup: HashMap<TokenKind, ExpressionParser>,
    pub lefthand_expression_lookup: HashMap<TokenKind, LefthandExpressionParser>,
    pub binding_power_lookup: HashMap<TokenKind, (u8, u8)>,
    pub type_lookup: HashMap<TokenKind, TypeParser>,
}

impl Lookup {
    pub fn add_expression<T, E>(mut self, token: TokenKind, expression_type: E) -> Self
    where
        T: Parse<Params = ()> + 'static,
        E: Fn(T) -> Expression<Untyped> + 'static,
    {
        if self.expression_lookup.contains_key(&token) {
            panic!("Token {:?} already has a prefix handler", token);
        }

        self.expression_lookup
            .insert(token, wrap_expression(expression_type));
        self
    }

    pub fn add_lefthand_expression<T, E>(
        mut self,
        token: TokenKind,
        binding_power: (u8, u8),
        expression_type: E,
    ) -> Self
    where
        T: Parse<Params = Expression<Untyped>> + 'static,
        E: Fn(T) -> Expression<Untyped> + 'static,
    {
        if self.lefthand_expression_lookup.contains_key(&token) {
            panic!("Token {:?} already has an infix handler", token);
        }

        self.lefthand_expression_lookup
            .insert(token.clone(), wrap_lefthand_expression(expression_type));
        self.binding_power_lookup.insert(token, binding_power);
        self
    }

    pub fn add_statement<T, E>(mut self, token: TokenKind, statement_type: E) -> Self
    where
        T: Parse<Params = ()> + 'static,
        E: Fn(T) -> Statement<Untyped> + 'static,
    {
        if self.statement_lookup.contains_key(&token) {
            panic!("Token {:?} already has a prefix handler", token);
        }

        self.statement_lookup
            .insert(token, wrap_statement(statement_type));
        self
    }

    pub fn add_type<T, E>(mut self, token: TokenKind, type_type: E) -> Self
    where
        T: Parse<Params = ()> + 'static,
        E: Fn(T) -> Type + 'static,
    {
        if self.type_lookup.contains_key(&token) {
            panic!("Token {:?} already has a type handler", token);
        }

        self.type_lookup.insert(token, wrap_type(type_type));
        self
    }
}

fn wrap_expression<T, E>(expression: E) -> ExpressionParser
where
    T: Parse<Params = ()> + 'static,
    E: Fn(T) -> Expression<Untyped> + 'static,
{
    Rc::new(move |input: &mut Parser| Ok(expression(input.parse::<T>()?)))
}

fn wrap_lefthand_expression<T, E>(expression: E) -> LefthandExpressionParser
where
    T: Parse<Params = Expression<Untyped>> + 'static,
    E: Fn(T) -> Expression<Untyped> + 'static,
{
    Rc::new(move |input: &mut Parser, lhs| Ok(expression(input.parse_with::<T>(lhs)?)))
}

fn wrap_statement<T, S>(statement: S) -> StatementParser
where
    T: Parse<Params = ()> + 'static,
    S: Fn(T) -> Statement<Untyped> + 'static,
{
    Rc::new(move |input: &mut Parser| Ok(statement(input.parse::<T>()?)))
}

fn wrap_type<T, S>(ty: S) -> TypeParser
where
    T: Parse<Params = ()> + 'static,
    S: Fn(T) -> Type + 'static,
{
    Rc::new(move |input: &mut Parser| Ok(ty(input.parse::<T>()?)))
}

impl Default for Lookup {
    fn default() -> Self {
        Lookup {
            statement_lookup: HashMap::new(),
            expression_lookup: HashMap::new(),
            lefthand_expression_lookup: HashMap::new(),
            binding_power_lookup: HashMap::new(),
            type_lookup: HashMap::new(),
        }
        .add_expression(TokenKind::Minus, Expression::Unary)
        .add_expression(TokenKind::Bang, Expression::Unary)
        .add_expression(TokenKind::Boolean, Expression::Primary)
        .add_expression(TokenKind::I32, Expression::Primary)
        .add_expression(TokenKind::I64, Expression::Primary)
        .add_expression(TokenKind::Decimal, Expression::Primary)
        .add_expression(TokenKind::String, Expression::Primary)
        .add_expression(TokenKind::Character, Expression::Primary)
        .add_expression(TokenKind::Identifier, Expression::Primary)
        .add_expression(TokenKind::ParenOpen, Expression::Group)
        .add_lefthand_expression(
            TokenKind::Plus,
            Precedence::Additive.left(),
            Expression::Binary,
        )
        .add_lefthand_expression(
            TokenKind::Minus,
            Precedence::Additive.left(),
            Expression::Binary,
        )
        .add_lefthand_expression(
            TokenKind::Star,
            Precedence::Multiplicative.left(),
            Expression::Binary,
        )
        .add_lefthand_expression(
            TokenKind::Slash,
            Precedence::Multiplicative.left(),
            Expression::Binary,
        )
        .add_lefthand_expression(
            TokenKind::LessThan,
            Precedence::Comparison.left(),
            Expression::Binary,
        )
        .add_lefthand_expression(
            TokenKind::LessThanOrEqual,
            Precedence::Comparison.left(),
            Expression::Binary,
        )
        .add_lefthand_expression(
            TokenKind::GreaterThan,
            Precedence::Comparison.left(),
            Expression::Binary,
        )
        .add_lefthand_expression(
            TokenKind::GreaterThanOrEqual,
            Precedence::Comparison.left(),
            Expression::Binary,
        )
        .add_lefthand_expression(
            TokenKind::Equality,
            Precedence::Equality.left(),
            Expression::Binary,
        )
        .add_lefthand_expression(
            TokenKind::Inequality,
            Precedence::Equality.left(),
            Expression::Binary,
        )
        .add_statement(TokenKind::CurlyOpen, Statement::Scope)
        .add_expression(TokenKind::CurlyOpen, Expression::Block)
        .add_statement(TokenKind::Let, Statement::Declaration)
        .add_lefthand_expression(
            TokenKind::If,
            Precedence::Ternary.left(),
            Expression::Ternary,
        )
        .add_lefthand_expression(
            TokenKind::ParenOpen,
            Precedence::Calling.left(),
            Expression::Call,
        )
        .add_expression(TokenKind::Function, Expression::Lambda)
        .add_statement(TokenKind::Type, Statement::TypeDefinition)
        // TODO: Add struct type parser
        // .add_type(TokenKind::CurlyOpen, Type::Struct)
    }
}

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum Precedence {
    Ternary,
    Equality,
    Comparison,
    Additive,
    Multiplicative,
    Calling,
}

impl Precedence {
    pub fn as_u8(&self) -> u8 {
        (*self as u8 + 1) * 2
    }

    pub fn left(&self) -> (u8, u8) {
        let n = self.as_u8();
        (n, n + 1)
    }

    pub fn right(&self) -> (u8, u8) {
        let n = self.as_u8();
        (n + 1, n)
    }
}
