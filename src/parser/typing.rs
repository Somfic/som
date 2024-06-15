use std::collections::HashMap;

use super::{
    ast::Type,
    lookup::BindingPower,
    macros::{expect_optional_token, expect_tokens, expect_type, expect_valid_token},
    Parser,
};
use crate::{
    diagnostic::Error,
    scanner::lexeme::{Lexeme, TokenType, TokenValue},
};

pub fn parse<'a>(
    parser: &'a Parser<'a>,
    cursor: usize,
    binding_power: &BindingPower,
) -> Result<(Type, usize), Error<'a>> {
    let mut cursor = cursor;
    let (token, range) = expect_valid_token!(parser, cursor)?;
    let type_handler = parser
        .lookup
        .type_lookup
        .get(&token.token_type)
        .ok_or(Error::primary(
            parser.lexemes.get(cursor).unwrap().range().file_id,
            cursor,
            range.length,
            "Expected a type",
        ))?;

    let (mut left_hand_side, new_cursor) = type_handler(parser, cursor)?;

    cursor = new_cursor;

    while let Some(Lexeme::Valid(token)) = parser.lexemes.get(cursor) {
        let token_binding_power = parser
            .lookup
            .binding_power_lookup
            .get(&token.token_type)
            .unwrap_or(&BindingPower::None);

        if binding_power > token_binding_power {
            break;
        }

        let left_type_handler = match parser.lookup.left_type_lookup.get(&token.token_type) {
            Some(handler) => handler,
            None => break,
        };

        let (right_hand_side, new_cursor) =
            left_type_handler(parser, cursor, left_hand_side, token_binding_power)?;

        cursor = new_cursor;
        left_hand_side = right_hand_side;
    }

    Ok((left_hand_side, cursor))
}

pub fn parse_symbol<'a>(parser: &'a Parser, cursor: usize) -> Result<(Type, usize), Error<'a>> {
    let (identifier, cursor) = expect_tokens!(parser, cursor, TokenType::Identifier)?;
    let identifier = match &identifier[0].value {
        TokenValue::Identifier(identifier) => identifier,
        _ => panic!("expect_token! should only return identifiers"),
    };
    Ok((Type::Symbol(identifier.clone()), cursor))
}

pub fn parse_array<'a>(parser: &'a Parser<'a>, cursor: usize) -> Result<(Type, usize), Error<'a>> {
    let (_, cursor) = expect_tokens!(parser, cursor, TokenType::SquareOpen)?;
    let (element_type, cursor) = expect_type!(parser, cursor, &BindingPower::None)?;
    let (_, cursor) = expect_tokens!(parser, cursor, TokenType::SquareClose)?;
    Ok((Type::Array(Box::new(element_type)), cursor))
}

pub fn parse_tuple<'a>(parser: &'a Parser<'a>, cursor: usize) -> Result<(Type, usize), Error<'a>> {
    let (_, cursor) = expect_tokens!(parser, cursor, TokenType::CurlyOpen)?;
    let mut new_cursor = cursor;
    let mut members: HashMap<String, Type> = HashMap::new();

    while let Some(Lexeme::Valid(token)) = parser.lexemes.get(new_cursor) {
        let (member_name, member_type, cursor) = match token.token_type {
            TokenType::CurlyClose => break,
            _ => {
                if !members.is_empty() {
                    let (_, cursor) = expect_tokens!(parser, new_cursor, TokenType::Comma)?;
                    new_cursor = cursor;
                }

                let (colon, _) = expect_optional_token!(parser, new_cursor + 1, TokenType::Colon)?;

                match colon {
                    Some(_) => {
                        let (field_name, cursor) =
                            expect_tokens!(parser, new_cursor, TokenType::Identifier)?;
                        let field_name = match &field_name[0].value {
                            TokenValue::Identifier(field_name) => field_name.clone(),
                            _ => panic!("expect_token! should only return identifiers"),
                        };

                        let (_, cursor) = expect_tokens!(parser, cursor, TokenType::Colon)?;
                        let (field_type, cursor) =
                            expect_type!(parser, cursor, BindingPower::None)?;

                        (field_name, field_type, cursor)
                    }
                    None => {
                        let field_name = members.len().to_string();
                        let (field_type, cursor) =
                            expect_type!(parser, new_cursor, BindingPower::None)?;
                        (field_name, field_type, cursor)
                    }
                }
            }
        };

        // TODO: Check for duplicate member names
        members.insert(member_name, member_type);

        new_cursor = cursor;
    }

    let (_, cursor) = expect_tokens!(parser, new_cursor, TokenType::CurlyClose)?;

    Ok((Type::Tuple(members), cursor))
}
