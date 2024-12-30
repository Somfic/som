use crate::{
    lexer::{TokenKind, TokenValue},
    parser::{
        ast::{
            untyped::{Expression, ExpressionValue, Primitive},
            Spannable,
        },
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

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Integer(value)),
    ))
}

pub fn decimal<'de>(parser: &mut Parser) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Decimal, "expected a decimal")?;

    let value = match token.value {
        TokenValue::Decimal(v) => v,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Decimal(value)),
    ))
}

pub fn boolean<'de>(parser: &mut Parser) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Boolean, "expected a boolean")?;

    let value = match token.value {
        TokenValue::Boolean(v) => v,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Boolean(value)),
    ))
}

pub fn character<'de>(parser: &mut Parser) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Character, "expected a character")?;

    let value = match token.value {
        TokenValue::Character(v) => v,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Character(value)),
    ))
}

pub fn string<'de>(parser: &mut Parser<'de>) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::String, "expected a string")?;

    let value = match token.value {
        TokenValue::String(v) => v,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::String(value)),
    ))
}

pub fn identifier<'de>(parser: &mut Parser<'de>) -> Result<Expression<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Identifier, "expected an identifier")?;

    let value = match token.value {
        TokenValue::Identifier(v) => v,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Identifier(value)),
    ))
}
