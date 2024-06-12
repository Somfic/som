use super::{
    ast::Type,
    lookup::BindingPower,
    macros::{expect_token, expect_type, expect_valid_token},
    Diagnostic, Parser,
};
use crate::scanner::lexeme::{Lexeme, TokenType, TokenValue};

pub fn parse(
    parser: &Parser,
    cursor: usize,
    binding_power: &BindingPower,
) -> Result<(Type, usize), Diagnostic> {
    let mut cursor = cursor;
    let (token, range) = expect_valid_token!(parser, cursor);
    let type_handler =
        parser
            .lookup
            .type_lookup
            .get(&token.token_type)
            .ok_or(Diagnostic::error(
                range,
                format!("No type handler for {:?}", token.token_type),
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

pub fn parse_symbol(parser: &Parser, cursor: usize) -> Result<(Type, usize), Diagnostic> {
    let (identifier, cursor) = expect_token!(parser, cursor, TokenType::Identifier)?;
    let identifier = match &identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => panic!("expect_token! should only return identifiers"),
    };
    Ok((Type::Symbol(identifier.clone()), cursor))
}

pub fn parse_array(parser: &Parser, cursor: usize) -> Result<(Type, usize), Diagnostic> {
    let (_, cursor) = expect_token!(parser, cursor, TokenType::SquareOpen)?;
    let (element_type, cursor) = expect_type!(parser, cursor, &BindingPower::None)?;
    let (_, cursor) = expect_token!(parser, cursor, TokenType::SquareClose)?;
    Ok((Type::Array(Box::new(element_type)), cursor))
}
