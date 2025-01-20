use crate::{
    ast::{Expression, ExpressionValue, Primitive, Spannable},
    lexer::{TokenKind, TokenValue},
    parser::Parser,
};
use miette::Result;

pub fn integer<'ast>(parser: &mut Parser) -> Result<Expression<'ast>> {
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

pub fn decimal<'ast>(parser: &mut Parser) -> Result<Expression<'ast>> {
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

pub fn boolean<'ast>(parser: &mut Parser) -> Result<Expression<'ast>> {
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

pub fn character<'ast>(parser: &mut Parser) -> Result<Expression<'ast>> {
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

pub fn string<'ast>(parser: &mut Parser<'ast>) -> Result<Expression<'ast>> {
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

pub fn identifier<'ast>(parser: &mut Parser<'ast>) -> Result<Expression<'ast>> {
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
