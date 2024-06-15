use super::{
    ast::{Expression, UnaryOperation},
    lookup::BindingPower,
    macros::{expect_expression, expect_token_value, expect_tokens, expect_valid_token},
    Parser,
};
use crate::{
    diagnostic::Error,
    parser::macros::expect_any_token,
    scanner::lexeme::{Lexeme, TokenType, TokenValue},
};
use std::collections::HashMap;

pub fn parse<'a>(
    parser: &'a Parser<'a>,
    cursor: usize,
    binding_power: &BindingPower,
) -> Result<(Expression, usize), Error<'a>> {
    let mut cursor = cursor;
    let (token, range) = expect_valid_token!(parser, cursor)?;
    let expression_handler = parser
        .lookup
        .expression_lookup
        .get(&token.token_type)
        .ok_or(Error::primary(
            range.file_id,
            cursor,
            range.length,
            "Expected a new expression",
        ))?;

    let (mut left_hand_side, new_cursor) = expression_handler(parser, cursor)?;

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

        let left_expression_handler =
            match parser.lookup.left_expression_lookup.get(&token.token_type) {
                Some(handler) => handler,
                None => break,
            };

        let (right_hand_side, new_cursor) =
            left_expression_handler(parser, cursor, left_hand_side, token_binding_power)?;

        cursor = new_cursor;
        left_hand_side = right_hand_side;
    }

    Ok((left_hand_side, cursor))
}

pub fn parse_assignment<'a>(
    parser: &'a Parser<'a>,
    cursor: usize,
    lhs: Expression,
    binding_power: &BindingPower,
) -> Result<(Expression, usize), Error<'a>> {
    let (_, cursor) = expect_tokens!(parser, cursor, TokenType::Equal)?;
    let (rhs, cursor) = expect_expression!(parser, cursor, binding_power)?;

    Ok((Expression::Assignment(Box::new(lhs), Box::new(rhs)), cursor))
}

pub fn parse_unary<'a>(
    parser: &'a Parser<'a>,
    cursor: usize,
) -> Result<(Expression, usize), Error<'a>> {
    let (token, cursor) = expect_any_token!(parser, cursor, TokenType::Minus, TokenType::Not)?;
    match token.token_type {
        TokenType::Minus => {
            let (expression, cursor) = expect_expression!(parser, cursor, &BindingPower::Unary)?;
            Ok((
                Expression::Unary(UnaryOperation::Negate, Box::new(expression)),
                cursor,
            ))
        }
        TokenType::Not => {
            let (expression, cursor) = expect_expression!(parser, cursor, &BindingPower::Unary)?;
            Ok((
                Expression::Unary(UnaryOperation::Inverse, Box::new(expression)),
                cursor,
            ))
        }
        _ => unreachable!(),
    }
}

pub fn parse_struct_initializer<'a>(
    parser: &'a Parser<'a>,
    cursor: usize,
    lhs: Expression,
    binding_power: &BindingPower,
) -> Result<(Expression, usize), Error<'a>> {
    let identifier = match lhs {
        Expression::Identifier(identifier) => identifier.clone(),
        _ => {
            unreachable!()
        }
    };

    let (_, cursor) = expect_tokens!(parser, cursor, TokenType::CurlyOpen)?;

    let mut members = HashMap::new();
    let mut new_cursor = cursor;

    while let Some(Lexeme::Valid(token)) = parser.lexemes.get(new_cursor) {
        if token.token_type == TokenType::CurlyClose {
            break;
        }

        if !members.is_empty() {
            let (_, cursor) = expect_tokens!(parser, new_cursor, TokenType::Comma)?;
            new_cursor = cursor;
        }

        let (tokens, cursor) =
            expect_tokens!(parser, new_cursor, TokenType::Identifier, TokenType::Colon)?;

        let identifier = expect_token_value!(tokens[0], TokenValue::Identifier);

        let (expression, cursor) = expect_expression!(parser, cursor, binding_power)?;

        members.insert(identifier, expression);

        new_cursor = cursor;
    }

    let (_, cursor) = expect_tokens!(parser, new_cursor, TokenType::CurlyClose)?;

    Ok((Expression::StructInitializer(identifier, members), cursor))
}
