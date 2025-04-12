use crate::{
    ast::{Typing, TypingValue},
    tokenizer::{TokenKind, TokenValue},
    ParserResult,
};

use super::Parser;

pub fn parse_symbol<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Typing<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Identifier, "expected a type")?;

    let name = match token.value {
        TokenValue::Identifier(name) => name,
        _ => unreachable!(),
    };

    Ok(Typing {
        value: TypingValue::Symbol(name),
        span: token.span,
    })
}

pub fn parse_integer<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Typing<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::IntegerType, "expected an integer type")?;

    Ok(Typing {
        value: TypingValue::Integer,
        span: token.span,
    })
}

pub fn parse_boolean<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Typing<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::BooleanType, "expected a boolean type")?;

    Ok(Typing {
        value: TypingValue::Boolean,
        span: token.span,
    })
}

pub fn parse_unit<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Typing<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::UnitType, "expected an unit type")?;

    Ok(Typing {
        value: TypingValue::Unit,
        span: token.span,
    })
}
