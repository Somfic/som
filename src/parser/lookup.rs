use super::{expression, statement, typing, Parser};
use crate::{
    ast::{Expression, ExpressionValue, Primitive, Spannable, Statement, StatementValue, Type},
    lexer::TokenKind,
};
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

pub type TypeHandler<'ast> = fn(&mut Parser<'ast>) -> Result<Type<'ast>>;
pub type LeftTypeHandler<'ast> = fn(&mut Parser<'ast>, Type, BindingPower) -> Result<Type<'ast>>;
pub type StatementHandler<'ast> =
    fn(&mut Parser<'ast>) -> Result<Statement<'ast, Expression<'ast>>>;
pub type ExpressionHandler<'ast> = fn(&mut Parser<'ast>) -> Result<Expression<'ast>>;
pub type LeftExpressionHandler<'ast> =
    fn(&mut Parser<'ast>, Expression<'ast>, BindingPower) -> Result<Expression<'ast>>;

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
        .add_expression_handler(TokenKind::Integer, expression::primitive::integer)
        .add_expression_handler(TokenKind::Decimal, expression::primitive::decimal)
        .add_expression_handler(TokenKind::Boolean, expression::primitive::boolean)
        .add_expression_handler(TokenKind::Character, expression::primitive::character)
        .add_expression_handler(TokenKind::String, expression::primitive::string)
        .add_expression_handler(TokenKind::Identifier, expression::primitive::identifier)
        .add_expression_handler(TokenKind::ParenOpen, group)
        .add_left_expression_handler(TokenKind::If, BindingPower::Logical, conditional)
        .add_left_expression_handler(TokenKind::ParenOpen, BindingPower::Call, expression::call)
        .add_expression_handler(TokenKind::Not, expression::unary::negate)
        .add_expression_handler(TokenKind::Minus, expression::unary::negative)
        .add_expression_handler(TokenKind::CurlyOpen, block)
        .add_expression_handler(TokenKind::Pipe, expression::lambda)
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
        .add_statement_handler(TokenKind::Let, statement::parse_declaration)
        .add_statement_handler(TokenKind::Type, statement::parse_type)
        .add_statement_handler(TokenKind::Struct, statement::parse_struct)
        .add_statement_handler(TokenKind::Enum, statement::parse_enum)
        .add_statement_handler(TokenKind::Function, statement::parse_function)
        .add_statement_handler(TokenKind::Trait, statement::parse_trait)
        .add_type_handler(TokenKind::Identifier, typing::parse_identifier)
        .add_type_handler(TokenKind::ParenOpen, typing::parse_unit)
        .add_type_handler(TokenKind::CharacterType, typing::parse_character)
        .add_type_handler(TokenKind::BooleanType, typing::parse_boolean)
        .add_type_handler(TokenKind::IntegerType, typing::parse_integer)
        .add_type_handler(TokenKind::DecimalType, typing::parse_decimal)
        .add_type_handler(TokenKind::StringType, typing::parse_string)
        .add_type_handler(TokenKind::SquareOpen, typing::parse_collection)
        .add_type_handler(TokenKind::CurlyOpen, typing::parse_set)
        .add_type_handler(TokenKind::Function, typing::parse_function)
        .add_statement_handler(TokenKind::Return, statement::parse_return)
        .add_statement_handler(TokenKind::If, statement::parse_condition)
    }
}

fn conditional<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    binding_power: BindingPower,
) -> Result<Expression<'ast>> {
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

fn group<'ast>(parser: &mut Parser<'ast>) -> Result<Expression<'ast>> {
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

fn block<'ast>(parser: &mut Parser<'ast>) -> Result<Expression<'ast>> {
    let open = parser
        .lexer
        .expect(TokenKind::CurlyOpen, "expected a left curly brace")?;

    // A list of statements separated by semicolons. If the last statement is not ended with a semicolon, it is considered the return value.
    let mut statements = Vec::new();
    let mut last_is_return = true;

    loop {
        // Check if a closing curly brace is found.
        if parser.lexer.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::CurlyClose)
        }) {
            break;
        }

        // Expect a semicolon after each statement except the last one.
        if !statements.is_empty() {
            parser
                .lexer
                .expect(TokenKind::Semicolon, "expected a semicolon")?;
        }

        if parser.lexer.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::CurlyClose)
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
