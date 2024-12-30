use crate::parser::{
    ast::{
        untyped::{BinaryOperator, Expression, ExpressionValue},
        Spannable,
    },
    lookup::BindingPower,
    Parser,
};
use miette::Result;

pub fn parse_binary_expression<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
    operator: BinaryOperator,
) -> Result<Expression<'de>> {
    let rhs = crate::parser::expression::parse(parser, bp)?;

    Ok(Expression::at_multiple(
        vec![rhs.span, lhs.span],
        ExpressionValue::Binary {
            operator,
            left: Box::new(lhs),
            right: Box::new(rhs),
        },
    ))
}

pub fn addition<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Add)
}

pub fn multiplication<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Multiply)
}

pub fn subtraction<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Subtract)
}

pub fn division<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Divide)
}

pub fn modulo<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Modulo)
}

pub fn equal<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Equality)
}

pub fn not_equal<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Inequality)
}

pub fn less_than<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::LessThan)
}

pub fn less_than_or_equal<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::LessThanOrEqual)
}

pub fn greater_than<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::GreaterThan)
}

pub fn greater_than_or_equal<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::GreaterThanOrEqual)
}

pub fn and<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::And)
}

pub fn or<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    bp: BindingPower,
) -> Result<Expression<'de>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Or)
}
