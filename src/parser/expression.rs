use super::{lookup::BindingPower, Parser};
use crate::ast::{
    BinaryOperator, Expression, ExpressionValue, Primitive, Spannable, UnaryOperator,
};
use crate::prelude::*;
use crate::tokenizer::{TokenKind, TokenValue};

pub fn parse_integer<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Integer, "expected an integer")?;

    let value = match token.value {
        TokenValue::Integer(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Integer(value)),
    ))
}

pub fn parse_decimal<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Decimal, "expected a decimal")?;

    let value = match token.value {
        TokenValue::Decimal(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Decimal(value)),
    ))
}

fn parse_binary_expression<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
    operator: BinaryOperator,
) -> ParserResult<Expression<'ast>> {
    let rhs = parser.parse_expression(bp)?;

    Ok(Expression::at_multiple(
        vec![rhs.span, lhs.span],
        ExpressionValue::Binary {
            operator,
            left: Box::new(lhs),
            right: Box::new(rhs),
        },
    ))
}

pub fn parse_binary_plus<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Add)
}

pub fn parse_binary_subtract<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Subtract)
}

pub fn parse_binary_multiply<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Multiply)
}

pub fn parse_binary_divide<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Divide)
}

pub fn parse_group<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::ParenOpen, "expected the start of the grouping")?;

    let expression = parser.parse_expression(BindingPower::None)?;

    parser
        .tokens
        .expect(TokenKind::ParenClose, "expected the end of the grouping")?;

    Ok(Expression::at_multiple(
        vec![token.span, expression.span],
        ExpressionValue::Group(Box::new(expression)),
    ))
}

pub fn parse_unary_negative<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Minus, "expected a negative sign")?;

    let expression = parser.parse_expression(BindingPower::Unary)?;

    Ok(Expression::at_multiple(
        vec![token.span, expression.span],
        ExpressionValue::Unary {
            operator: UnaryOperator::Negative,
            operand: Box::new(expression),
        },
    ))
}

pub fn parse_boolean<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Boolean, "expected a boolean")?;

    let value = match token.value {
        TokenValue::Boolean(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Boolean(value)),
    ))
}

pub fn parse_conditional<'ast>(
    parser: &mut Parser<'ast>,
    truthy: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    let condition = parser.parse_expression(BindingPower::None)?;

    parser.tokens.expect(TokenKind::Else, "expected an else")?;

    let falsy = parser.parse_expression(bp)?;

    Ok(Expression::at_multiple(
        vec![condition.span, truthy.span, falsy.span],
        ExpressionValue::Conditional {
            condition: Box::new(condition),
            truthy: Box::new(truthy),
            falsy: Box::new(falsy),
        },
    ))
}
