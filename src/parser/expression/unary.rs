use crate::{
    lexer::TokenKind,
    parser::{
        ast::{
            untyped::{Expression, ExpressionValue, UnaryOperator},
            Spannable,
        },
        lookup::BindingPower,
        Parser,
    },
};
use miette::Result;

pub fn negate<'de>(parser: &mut Parser<'de>) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Not, "expected a negate operator")?;
    let expression = crate::parser::expression::parse(parser, BindingPower::None)?;

    Ok(Expression::at_multiple(
        vec![token.span, expression.span],
        ExpressionValue::Unary {
            operator: UnaryOperator::Negate,
            operand: Box::new(expression),
        },
    ))
}

pub fn negative<'de>(parser: &mut Parser<'de>) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Minus, "expected a negative operator")?;
    let expression = crate::parser::expression::parse(parser, BindingPower::None)?;

    Ok(Expression::at_multiple(
        vec![token.span, expression.span],
        ExpressionValue::Unary {
            operator: UnaryOperator::Negative,
            operand: Box::new(expression),
        },
    ))
}
