use crate::{
    diagnostic::{Diagnostic, Snippet},
    scanner::lexeme::TokenType,
};

use super::{
    ast::Type,
    lookup::{BindingPower, Lookup},
    macros::{expect_token, expect_value},
    ParseResult, Parser,
};

pub fn register(lookup: &mut Lookup) {
    lookup.add_type_handler(TokenType::Identifier, parse_symbol);
    lookup.add_type_handler(TokenType::SquareOpen, parse_array);
}

pub fn parse<'a>(parser: &mut Parser<'a>, binding_power: BindingPower) -> ParseResult<'a, Type> {
    let token = expect_token!(parser)?;

    let type_handler = parser.lookup.type_lookup.get(&token.token_type).ok_or(
        Diagnostic::error("P0001", "Expected a new type").with_snippet(
            Snippet::primary_from_token(token, "Expected a type to start here"),
        ),
    )?;

    let mut left_hand_side = type_handler(parser)?;

    while parser.has_tokens() {
        let token = parser.peek().unwrap();

        let token_binding_power = parser
            .lookup
            .binding_power_lookup
            .get(&parser.peek().unwrap().token_type)
            .copied()
            .unwrap_or_default();

        if binding_power > token_binding_power {
            break;
        }

        let left_type_handler = match parser.lookup.left_type_lookup.get(&token.token_type) {
            Some(handler) => handler,
            None => break,
        };

        left_hand_side = left_type_handler(parser, left_hand_side, token_binding_power)?;
    }

    Ok(left_hand_side)
}

pub fn parse_symbol<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Type> {
    let identifier = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier, Identifier);

    Ok(Type::Symbol(identifier))
}

pub fn parse_array<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Type> {
    expect_token!(parser, SquareOpen)?;

    let element_type = parse(parser, BindingPower::None)?;

    expect_token!(parser, SquareClose)?;

    Ok(Type::Array(Box::new(element_type)))
}
