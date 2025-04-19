use super::{BindingPower, Parser};
use crate::ast::{CombineSpan, Identifier, StructMember};
use crate::tokenizer::Token;
use crate::{
    ast::{Typing, TypingValue},
    tokenizer::{TokenKind, TokenValue},
    ParserResult,
};

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

pub fn parse_generic<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Typing<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Tick, "expected a generic type")?;

    let identifier = parser
        .tokens
        .expect(TokenKind::Identifier, "expected a type")?;

    let identifier_name = match identifier.value {
        TokenValue::Identifier(name) => name,
        _ => unreachable!(),
    };

    let span = token.span.combine(identifier.span);

    Ok(TypingValue::Generic(identifier_name).with_span(span))
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

pub fn parse_struct<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
) -> ParserResult<Typing<'ast>> {
    parser
        .tokens
        .expect(TokenKind::CurlyOpen, "expected a struct type")?;

    let mut fields = Vec::new();

    while let Some(token) = parser.tokens.peek() {
        if token
            .as_ref()
            .is_ok_and(|t| t.kind == TokenKind::CurlyClose)
        {
            break;
        }

        if !fields.is_empty() {
            parser.tokens.expect(TokenKind::Comma, "expected a comma")?;
        }

        let identifier = parser
            .tokens
            .expect(TokenKind::Identifier, "expected a struct member")?;

        let identifier_name = match identifier.value {
            TokenValue::Identifier(name) => name,
            _ => unreachable!(),
        };

        parser.tokens.expect(TokenKind::Tilde, "expected a type")?;

        let ty = parser.parse_typing(BindingPower::None)?;

        let field = StructMember {
            name: identifier_name,
            ty,
        };
        fields.push(field);
    }

    parser
        .tokens
        .expect(TokenKind::CurlyClose, "expected a closing curly bracket")?;

    Ok(TypingValue::Struct(fields).with_span(identifier.span))
}
