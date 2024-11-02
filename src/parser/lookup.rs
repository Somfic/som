use super::{
    ast::{Expression, ExpressionValue, Primitive, Spannable, Statement, StatementValue, Type},
    expression, statement, typing, Parser,
};
use crate::lexer::{TokenKind, TokenValue};
use miette::{Context, Result, SourceSpan};
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

pub type TypeHandler<'de> = fn(&mut Parser<'de>) -> Result<Type<'de>>;
pub type LeftTypeHandler<'de> = fn(&mut Parser<'de>, Type, BindingPower) -> Result<Type<'de>>;
pub type StatementHandler<'de> = fn(&mut Parser<'de>) -> Result<Statement<'de>>;
pub type ExpressionHandler<'de> = fn(&mut Parser<'de>) -> Result<Expression<'de>>;
pub type LeftExpressionHandler<'de> =
    fn(&mut Parser<'de>, Expression<'de>, BindingPower) -> Result<Expression<'de>>;

pub struct Lookup<'de> {
    pub statement_lookup: HashMap<TokenKind, StatementHandler<'de>>,
    pub expression_lookup: HashMap<TokenKind, ExpressionHandler<'de>>,
    pub left_expression_lookup: HashMap<TokenKind, LeftExpressionHandler<'de>>,
    pub type_lookup: HashMap<TokenKind, TypeHandler<'de>>,
    pub left_type_lookup: HashMap<TokenKind, LeftTypeHandler<'de>>,
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

    pub(crate) fn add_type_handler(mut self, token: TokenKind, handler: TypeHandler<'de>) -> Self {
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
        handler: LeftTypeHandler<'de>,
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
        .add_expression_handler(TokenKind::Integer, expression::primitive::integer)
        .add_expression_handler(TokenKind::Decimal, expression::primitive::decimal)
        .add_expression_handler(TokenKind::Boolean, expression::primitive::boolean)
        .add_expression_handler(TokenKind::Character, expression::primitive::character)
        .add_expression_handler(TokenKind::String, expression::primitive::string)
        .add_expression_handler(TokenKind::Identifier, expression::primitive::identifier)
        .add_expression_handler(TokenKind::ParenOpen, group)
        .add_left_expression_handler(TokenKind::If, BindingPower::None, conditional)
        .add_left_expression_handler(TokenKind::ParenOpen, BindingPower::None, expression::call)
        .add_expression_handler(TokenKind::Not, expression::unary::negate)
        .add_expression_handler(TokenKind::Minus, expression::unary::negative)
        .add_expression_handler(TokenKind::CurlyOpen, block)
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
        .add_left_expression_handler(
            TokenKind::Equality,
            BindingPower::Assignment,
            expression::binary::equal,
        )
        .add_left_expression_handler(
            TokenKind::Inequality,
            BindingPower::Assignment,
            expression::binary::not_equal,
        )
        .add_left_expression_handler(
            TokenKind::LessThan,
            BindingPower::Relational,
            expression::binary::less_than,
        )
        .add_left_expression_handler(
            TokenKind::LessThanOrEqual,
            BindingPower::Relational,
            expression::binary::less_than_or_equal,
        )
        .add_left_expression_handler(
            TokenKind::GreaterThan,
            BindingPower::Relational,
            expression::binary::greater_than,
        )
        .add_left_expression_handler(
            TokenKind::GreaterThanOrEqual,
            BindingPower::Relational,
            expression::binary::greater_than_or_equal,
        )
        .add_left_expression_handler(
            TokenKind::Percent,
            BindingPower::Multiplicative,
            expression::binary::modulo,
        )
        .add_left_expression_handler(
            TokenKind::And,
            BindingPower::Relational,
            expression::binary::and,
        )
        .add_left_expression_handler(
            TokenKind::Or,
            BindingPower::Relational,
            expression::binary::or,
        )
        .add_statement_handler(TokenKind::Let, statement::let_)
        .add_statement_handler(TokenKind::Struct, statement::struct_)
        .add_statement_handler(TokenKind::Enum, statement::enum_)
        .add_statement_handler(TokenKind::Function, statement::function_)
        .add_statement_handler(TokenKind::Trait, statement::trait_)
        .add_type_handler(TokenKind::Identifier, typing::identifier)
        .add_type_handler(TokenKind::ParenOpen, typing::unit)
        .add_type_handler(TokenKind::CharacterType, typing::character)
        .add_type_handler(TokenKind::BooleanType, typing::boolean)
        .add_type_handler(TokenKind::IntegerType, typing::integer)
        .add_type_handler(TokenKind::DecimalType, typing::decimal)
        .add_type_handler(TokenKind::StringType, typing::string)
        .add_type_handler(TokenKind::SquareOpen, typing::collection)
        .add_type_handler(TokenKind::CurlyOpen, typing::set)
        .add_statement_handler(TokenKind::Return, statement::return_)
        .add_statement_handler(TokenKind::If, statement::if_)
    }
}

fn conditional<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    binding_power: BindingPower,
) -> Result<Expression<'de>> {
    let condition = expression::parse(parser, binding_power.clone())?;

    let token = parser
        .lexer
        .expect(TokenKind::Else, "expected an else branch")?;

    let falsy = expression::parse(parser, binding_power)?;

    Ok(Expression::at_multiple(
        vec![condition.span, token.span, falsy.span],
        ExpressionValue::Conditional {
            condition: Box::new(condition),
            truthy: Box::new(lhs),
            falsy: Box::new(falsy),
        },
    ))
}

fn group<'de>(parser: &mut Parser<'de>) -> Result<Expression<'de>> {
    let open = parser
        .lexer
        .expect(TokenKind::ParenOpen, "expected a left parenthesis")?;
    let expression = expression::parse(parser, BindingPower::None)?;
    let close = parser
        .lexer
        .expect(TokenKind::ParenClose, "expected a right parenthesis")?;

    Ok(Expression::at_multiple(
        vec![open.span, expression.span, close.span],
        ExpressionValue::Group(Box::new(expression)),
    ))
}

fn block<'de>(parser: &mut Parser<'de>) -> Result<Expression<'de>> {
    let open = parser
        .lexer
        .expect(TokenKind::CurlyOpen, "expected a left curly brace")?;

    // A list of statements separated by semicolons. If the last statement is not ended with a semicolon, it is considered the return value.
    let mut statements = Vec::new();
    let mut last_is_return = true;

    loop {
        // Check if a closing curly brace is found.
        if parser.lexer.peek().map_or(false, |token| {
            token
                .as_ref()
                .map_or(false, |token| token.kind == TokenKind::CurlyClose)
        }) {
            break;
        }

        // Expect a semicolon after each statement except the last one.
        if !statements.is_empty() {
            parser
                .lexer
                .expect(TokenKind::Semicolon, "expected a semicolon")?;
        }

        if parser.lexer.peek().map_or(false, |token| {
            token
                .as_ref()
                .map_or(false, |token| token.kind == TokenKind::CurlyClose)
        }) {
            last_is_return = false;
            break;
        }

        let statement =
            crate::parser::statement::parse(parser, true).wrap_err("while parsing block")?;
        statements.push(statement);
    }

    let return_value = if last_is_return {
        match statements.last().map(|s| &s.value) {
            Some(StatementValue::Expression(_)) => match statements.pop().map(|s| s.value) {
                Some(StatementValue::Expression(expression)) => expression,
                _ => unreachable!(),
            },
            _ => Expression::at(
                statements
                    .last()
                    .map_or(SourceSpan::new(0.into(), 0), |s| s.span),
                ExpressionValue::Primitive(Primitive::Unit),
            ),
        }
    } else {
        Expression::at(
            statements
                .last()
                .map_or(SourceSpan::new(0.into(), 0), |s| s.span),
            ExpressionValue::Primitive(Primitive::Unit),
        )
    };

    let expression = Expression::at(
        return_value.span,
        ExpressionValue::Block {
            statements,
            return_value: Box::new(return_value),
        },
    );

    let close = parser
        .lexer
        .expect(TokenKind::CurlyClose, "expected a right curly brace")?;

    Ok(Expression::at_multiple(
        vec![open.span, close.span],
        ExpressionValue::Group(Box::new(expression)),
    ))
}
