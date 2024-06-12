use std::collections::HashMap;

use super::{
    ast::{Expression, UnaryOperation},
    lookup::BindingPower,
    macros::{expect_expression, expect_token, expect_valid_token},
    Diagnostic, Parser,
};
use crate::{
    parser::macros::expect_any_token,
    scanner::lexeme::{Lexeme, Range, TokenType, TokenValue},
};

pub fn parse(
    parser: &Parser,
    cursor: usize,
    binding_power: &BindingPower,
) -> Result<(Expression, usize), Diagnostic> {
    let mut cursor = cursor;
    let (token, range) = expect_valid_token!(parser, cursor);
    let expression_handler = parser
        .lookup
        .expression_lookup
        .get(&token.token_type)
        .ok_or(Diagnostic::error(
            range,
            format!("No expression handler for {:?}", token.token_type),
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

pub fn parse_assignment(
    parser: &Parser,
    cursor: usize,
    lhs: Expression,
    binding_power: &BindingPower,
) -> Result<(Expression, usize), Diagnostic> {
    let (_, cursor) = expect_token!(parser, cursor, TokenType::Equal)?;
    let (rhs, cursor) = expect_expression!(parser, cursor, binding_power)?;

    Ok((Expression::Assignment(Box::new(lhs), Box::new(rhs)), cursor))
}

pub fn parse_unary(parser: &Parser, cursor: usize) -> Result<(Expression, usize), Diagnostic> {
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

pub fn parse_struct_initializer(
    parser: &Parser,
    cursor: usize,
    lhs: Expression,
    binding_power: &BindingPower,
) -> Result<(Expression, usize), Diagnostic> {
    let identifier = match lhs {
        Expression::Identifier(identifier) => identifier.clone(),
        _ => {
            return Err(Diagnostic::error(
                &Range {
                    position: cursor,
                    length: 0,
                },
                format!("Expected identifier, found {:?}", lhs),
            ))
        }
    };

    let (_, cursor) = expect_token!(parser, cursor, TokenType::CurlyOpen)?;

    let mut members = HashMap::new();
    let mut new_cursor = cursor;

    while let Some(Lexeme::Valid(token)) = parser.lexemes.get(new_cursor) {
        if token.token_type == TokenType::CurlyClose {
            break;
        }

        if !members.is_empty() {
            let (_, cursor) = expect_token!(parser, new_cursor, TokenType::Comma)?;
            new_cursor = cursor;
        }

        let (member, cursor) = expect_token!(parser, new_cursor, TokenType::Identifier)?;
        let member = match &member.value {
            TokenValue::Identifier(member) => member.clone(),
            _ => panic!("expect_token! should only return identifiers"),
        };

        let (_, cursor) = expect_token!(parser, cursor, TokenType::Colon)?;

        let (expression, cursor) = expect_expression!(parser, cursor, binding_power)?;

        members.insert(member, expression);

        new_cursor = cursor;
    }

    let (_, cursor) = expect_token!(parser, new_cursor, TokenType::CurlyClose)?;

    Ok((Expression::StructInitializer(identifier, members), cursor))
}
