use crate::{
    ast::{Expr, Expression, Statement},
    lexer::TokenKind,
    parser::{Parse, Parser, Untyped},
    Result,
};
use std::{collections::HashMap, rc::Rc};

pub type ExpressionParser = Rc<dyn Fn(&mut Parser) -> Result<Expression<Untyped>>>;
pub type LefthandExpressionParser =
    Rc<dyn Fn(&mut Parser, Expression<Untyped>) -> Result<Expression<Untyped>>>;
pub type StatementParser = Rc<dyn Fn(&mut Parser) -> Result<Statement<Untyped>>>;

pub struct Lookup {
    pub statement_lookup: HashMap<TokenKind, StatementParser>,
    pub expression_lookup: HashMap<TokenKind, ExpressionParser>,
    pub lefthand_expression_lookup: HashMap<TokenKind, LefthandExpressionParser>,
    pub binding_power_lookup: HashMap<TokenKind, (u8, u8)>,
}

impl Lookup {
    pub fn add_expression<T, E>(mut self, token: TokenKind, expression_type: E) -> Self
    where
        T: Parse<Params = ()> + 'static,
        E: Fn(T) -> Expr<Untyped> + 'static,
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
        E: Fn(T) -> Expr<Untyped> + 'static,
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
}

fn wrap_expression<T, E>(expression: E) -> ExpressionParser
where
    T: Parse<Params = ()> + 'static,
    E: Fn(T) -> Expr<Untyped> + 'static,
{
    Rc::new(move |input: &mut Parser| {
        let (inner, span) = input.parse_with_span::<T>()?;
        Ok(Expression {
            expr: expression(inner),
            span,
            ty: (),
        })
    })
}

fn wrap_lefthand_expression<T, E>(expression: E) -> LefthandExpressionParser
where
    T: Parse<Params = Expression<Untyped>> + 'static,
    E: Fn(T) -> Expr<Untyped> + 'static,
{
    Rc::new(move |input: &mut Parser, lhs: Expression<Untyped>| {
        let start_span = lhs.span.clone();
        let (inner, span) = input.parse_with_span_with::<T>(lhs)?;

        Ok(Expression {
            expr: expression(inner),
            span: &start_span + &span,
            ty: (),
        })
    })
}

fn wrap_statement<T, S>(statement: S) -> StatementParser
where
    T: Parse<Params = ()> + 'static,
    S: Fn(T) -> Statement<Untyped> + 'static,
{
    Rc::new(move |input: &mut Parser| Ok(statement(input.parse::<T>()?)))
}

impl Default for Lookup {
    fn default() -> Self {
        Lookup {
            statement_lookup: HashMap::new(),
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
        .add_statement(TokenKind::CurlyOpen, Statement::Scope)
        .add_expression(TokenKind::CurlyOpen, Expr::Block)
    }
}
