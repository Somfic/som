use super::{
    ast::{Spannable, Type, TypeValue},
    lookup::BindingPower,
    Parser,
};
use crate::lexer::{TokenKind, TokenValue};
use miette::Result;

pub fn parse<'de>(parser: &mut Parser<'de>, binding_power: BindingPower) -> Result<Type<'de>> {
    let token = match parser.lexer.peek().as_ref() {
        Some(Ok(token)) => token,
        Some(Err(err)) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
        None => {
            return Err(miette::miette! {
                help = "expected a type",
                "expected a type"
            })
        }
    };

    let handler = parser
        .lookup
        .type_lookup
        .get(&token.kind)
        .ok_or(miette::miette! {
            labels = vec![token.label("expected a type")],
            help = format!("{} is not a type", token.kind),
            "expected a type, found {}", token.kind
        })?;
    let mut lhs = handler(parser)?;

    let mut next_token = parser.lexer.peek();

    while let Some(token) = next_token {
        let token = match token {
            Ok(token) => token,
            Err(err) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
        };

        let token_binding_power = {
            let binding_power_lookup = parser.lookup.binding_power_lookup.clone();
            binding_power_lookup
                .get(&token.kind)
                .unwrap_or(&BindingPower::None)
                .clone()
        };

        if binding_power > token_binding_power {
            break;
        }

        let handler = match parser.lookup.left_type_lookup.get(&token.kind) {
            Some(handler) => handler,
            None => break,
        };

        parser.lexer.next();

        lhs = handler(parser, lhs, token_binding_power)?;

        next_token = parser.lexer.peek();
    }

    Ok(lhs)
}

pub fn unit<'de>(parser: &mut Parser<'de>) -> Result<Type<'de>> {
    let open = parser
        .lexer
        .expect(TokenKind::ParenOpen, "expected an opening parenthesis")?;

    let close = parser
        .lexer
        .expect(TokenKind::ParenClose, "expected a closing parenthesis")?;

    Ok(Type::at_multiple(
        vec![open.span, close.span],
        TypeValue::Unit,
    ))
}

pub fn boolean<'de>(parser: &mut Parser<'de>) -> Result<Type<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::BooleanType, "expected a boolean type")?;

    Ok(Type::at(token.span, TypeValue::Boolean))
}

pub fn integer<'de>(parser: &mut Parser<'de>) -> Result<Type<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::IntegerType, "expected an integer type")?;

    Ok(Type::at(token.span, TypeValue::Integer))
}

pub fn decimal<'de>(parser: &mut Parser<'de>) -> Result<Type<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::DecimalType, "expected a decimal type")?;

    Ok(Type::at(token.span, TypeValue::Decimal))
}

pub fn string<'de>(parser: &mut Parser<'de>) -> Result<Type<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::StringType, "expected a string type")?;

    Ok(Type::at(token.span, TypeValue::String))
}

pub fn character<'de>(parser: &mut Parser<'de>) -> Result<Type<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::CharacterType, "expected a character type")?;

    Ok(Type::at(token.span, TypeValue::Character))
}

pub fn collection<'de>(parser: &mut Parser<'de>) -> Result<Type<'de>> {
    let open = parser
        .lexer
        .expect(TokenKind::SquareOpen, "expected an opening bracket")?;
    let element = parse(parser, BindingPower::None)?;
    let close = parser
        .lexer
        .expect(TokenKind::SquareClose, "expected a closing bracket")?;

    Ok(Type::at_multiple(
        vec![open.span, element.span, close.span],
        TypeValue::Collection(Box::new(element)),
    ))
}

pub fn set<'de>(parser: &mut Parser<'de>) -> Result<Type<'de>> {
    let open = parser
        .lexer
        .expect(TokenKind::CurlyOpen, "expected an opening curly brace")?;

    let element = parse(parser, BindingPower::None)?;

    let close = parser
        .lexer
        .expect(TokenKind::CurlyClose, "expected a closing curly brace")?;

    Ok(Type::at_multiple(
        vec![open.span, element.span, close.span],
        TypeValue::Set(Box::new(element)),
    ))
}

pub fn identifier<'de>(parser: &mut Parser<'de>) -> Result<Type<'de>> {
    let token = parser
        .lexer
        .expect(TokenKind::Identifier, "expected an identifier")?;

    let name = match token.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    Ok(Type::at(token.span, TypeValue::Symbol(name)))
}
