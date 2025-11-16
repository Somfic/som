use crate::{
    ast::{Expr, Expression},
    lexer::TokenKind,
    parser::{Parse, Parser, Untyped},
    Result,
};
use std::{collections::HashMap, rc::Rc};

pub type PrefixParselet = Rc<dyn Fn(&mut Parser) -> Result<Expression<Untyped>>>;
pub type InfixParselet =
    Rc<dyn Fn(&mut Parser, Expression<Untyped>) -> Result<Expression<Untyped>>>;

pub struct Lookup {
    pub expression_lookup: HashMap<TokenKind, PrefixParselet>,
    pub lefthand_expression_lookup: HashMap<TokenKind, InfixParselet>,
    pub binding_power_lookup: HashMap<TokenKind, (u8, u8)>,
}

impl Lookup {
    pub fn add_expression<T, E>(mut self, token: TokenKind, expr_type: E) -> Self
    where
        T: Parse<Params = ()> + 'static,
        E: Fn(T) -> Expr<Untyped> + 'static,
    {
        if self.expression_lookup.contains_key(&token) {
            panic!("Token {:?} already has a prefix handler", token);
        }

        self.expression_lookup
            .insert(token, wrap_expression(expr_type));
        self
    }

    pub fn add_lefthand_expression<T, E>(
        mut self,
        token: TokenKind,
        binding_power: (u8, u8),
        expr: E,
    ) -> Self
    where
        T: Parse<Params = Expression<Untyped>> + 'static,
        E: Fn(T) -> Expr<Untyped> + 'static,
    {
        if self.lefthand_expression_lookup.contains_key(&token) {
            panic!("Token {:?} already has an infix handler", token);
        }

        self.lefthand_expression_lookup
            .insert(token.clone(), wrap_lefthand_expression(expr));
        self.binding_power_lookup.insert(token, binding_power);
        self
    }
}

fn wrap_expression<T, E>(expr: E) -> PrefixParselet
where
    T: Parse<Params = ()> + 'static,
    E: Fn(T) -> Expr<Untyped> + 'static,
{
    Rc::new(move |input: &mut Parser| {
        let (inner, span) = input.parse_with_span::<T>()?;
        Ok(Expression {
            expr: expr(inner),
            span,
            ty: (),
        })
    })
}

fn wrap_lefthand_expression<T, E>(expr: E) -> InfixParselet
where
    T: Parse<Params = Expression<Untyped>> + 'static,
    E: Fn(T) -> Expr<Untyped> + 'static,
{
    Rc::new(move |input: &mut Parser, lhs: Expression<Untyped>| {
        let start_span = lhs.span.clone();
        let (inner, span) = input.parse_with_span_with::<T>(lhs)?;

        Ok(Expression {
            expr: expr(inner),
            span: &start_span + &span,
            ty: (),
        })
    })
}

impl Default for Lookup {
    fn default() -> Self {
        Lookup {
            expression_lookup: HashMap::new(),
            lefthand_expression_lookup: HashMap::new(),
            binding_power_lookup: HashMap::new(),
        }
        .add_expression(TokenKind::Minus, Expr::Unary)
        .add_expression(TokenKind::Bang, Expr::Unary)
        .add_expression(TokenKind::Boolean, Expr::Primary)
        .add_expression(TokenKind::I32, Expr::Primary)
        .add_expression(TokenKind::I64, Expr::Primary)
        .add_expression(TokenKind::Decimal, Expr::Primary)
        .add_expression(TokenKind::String, Expr::Primary)
        .add_expression(TokenKind::Character, Expr::Primary)
        .add_expression(TokenKind::Identifier, Expr::Primary)
        .add_expression(TokenKind::ParenOpen, Expr::Group)
        .add_lefthand_expression(TokenKind::Plus, (9, 10), Expr::Binary)
        .add_lefthand_expression(TokenKind::Minus, (9, 10), Expr::Binary)
        .add_lefthand_expression(TokenKind::Star, (11, 12), Expr::Binary)
        .add_lefthand_expression(TokenKind::Slash, (11, 12), Expr::Binary)
    }
}
