use crate::{
    lexer::{TokenKind, TokenValue},
    parser::{
        ast::{Expression, Primitive},
        Parser,
    },
};
use miette::Result;

pub fn integer<'de>(parser: &mut Parser) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Integer, "expected an integer")?;

    let value = match token.value {
        TokenValue::Integer(v) => v,
        _ => unreachable!(),
    };

    Ok(Expression::Primitive(Primitive::Integer(value)))
}

pub fn decimal<'de>(parser: &mut Parser) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Decimal, "expected a decimal")?;

    let value = match token.value {
        TokenValue::Decimal(v) => v,
        _ => unreachable!(),
    };

    Ok(Expression::Primitive(Primitive::Decimal(value)))
}

pub fn boolean<'de>(parser: &mut Parser) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Boolean, "expected a boolean")?;

    let value = match token.value {
        TokenValue::Boolean(v) => v,
        _ => unreachable!(),
    };

    Ok(Expression::Primitive(Primitive::Boolean(value)))
}
