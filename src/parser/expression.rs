use super::{lookup::BindingPower, Parser};
use crate::ast::{BinaryOperator, Expression, ExpressionValue, Primitive, Spannable};
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
