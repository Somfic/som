use super::{
    ast::{
        Spannable, {Type, TypeValue},
    },
    lookup::BindingPower,
    Parser,
};
use crate::lexer::{TokenKind, TokenValue};
use miette::Result;

pub fn parse<'ast>(parser: &mut Parser<'ast>, binding_power: BindingPower) -> Result<Type<'ast>> {
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

pub fn unit<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
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

pub fn boolean<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
    let token = parser
        .lexer
        .expect(TokenKind::BooleanType, "expected a boolean type")?;

    Ok(Type::at(token.span, TypeValue::Boolean))
}

pub fn integer<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
    let token = parser
        .lexer
        .expect(TokenKind::IntegerType, "expected an integer type")?;

    Ok(Type::at(token.span, TypeValue::Integer))
}

pub fn decimal<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
    let token = parser
        .lexer
        .expect(TokenKind::DecimalType, "expected a decimal type")?;

    Ok(Type::at(token.span, TypeValue::Decimal))
}

pub fn string<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
    let token = parser
        .lexer
        .expect(TokenKind::StringType, "expected a string type")?;

    Ok(Type::string(token.span))
}

pub fn character<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
    let token = parser
        .lexer
        .expect(TokenKind::CharacterType, "expected a character type")?;

    Ok(Type::character(token.span))
}

pub fn collection<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
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

pub fn set<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
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

pub fn identifier<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
    let token = parser
        .lexer
        .expect(TokenKind::Identifier, "expected an identifier")?;

    let name = match token.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    Ok(Type::symbol(token.span, name))
}

pub fn function<'ast>(parser: &mut Parser<'ast>) -> Result<Type<'ast>> {
    let function = parser
        .lexer
        .expect(TokenKind::Function, "expected a function type")?;

    let open = parser
        .lexer
        .expect(TokenKind::ParenOpen, "expected an opening parenthesis")?;

    let mut parameters = Vec::new();

    while parser.lexer.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind != TokenKind::ParenClose)
    }) {
        if !parameters.is_empty() {
            parser
                .lexer
                .expect(TokenKind::Comma, "expected a comma between parameters")?;
        }

        let parameter = parse(parser, BindingPower::None)?;
        parameters.push(parameter);
    }

    let close = parser
        .lexer
        .expect(TokenKind::ParenClose, "expected a closing parenthesis")?;

    let explicit_return_type = match parser.lexer.peek_expect(TokenKind::Arrow) {
        None => None,
        Some(_) => {
            parser.lexer.next();
            Some(parse(parser, BindingPower::None)?)
        }
    };

    Ok(Type::at_multiple(
        vec![
            function.span,
            open.span,
            close.span,
            explicit_return_type.as_ref().map_or(open.span, |t| t.span),
        ],
        TypeValue::Function {
            parameters,
            return_type: Box::new(explicit_return_type.unwrap_or_else(|| Type::unit(open.span))),
        },
    ))
}
