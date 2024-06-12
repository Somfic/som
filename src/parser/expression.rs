use super::{
    ast::{Expression, UnaryOperation},
    lookup::BindingPower,
    macros::{expect_expression, expect_token},
    Diagnostic, Parser,
};
use crate::{
    parser::macros::expect_any_token,
    scanner::lexeme::{Lexeme, Range, Token, TokenType},
};

pub fn parse(
    parser: &Parser,
    cursor: usize,
    binding_power: &BindingPower,
) -> Result<(Expression, usize), Diagnostic> {
    let mut cursor = cursor;

    // Consume the current lexeme
    let lexeme = parser.lexemes.get(cursor);

    if lexeme.is_none() {
        return Err(Diagnostic::error(
            &Range {
                position: cursor,
                length: 0,
            },
            "Expected expression",
        ));
    }

    let lexeme = lexeme.unwrap();

    let token = match lexeme {
        Lexeme::Valid(token) => token,
        Lexeme::Invalid(_) => return Err(Diagnostic::error(lexeme.range(), "Invalid token")),
    };

    let expression_handler = parser.lookup.expression_lookup.get(&token.token_type);

    if expression_handler.is_none() {
        return Err(Diagnostic::error(
            lexeme.range(),
            "Expected expression".to_string(),
        ));
    }

    let expression_handler = expression_handler.unwrap();

    let (mut left_hand_side, new_cursor) = expression_handler(parser, cursor)?;
    cursor = new_cursor;

    while let Some(lexeme) = parser.lexemes.get(cursor) {
        let token = match lexeme {
            Lexeme::Valid(token) => token,
            Lexeme::Invalid(_) => break,
        };

        let token_binding_power = parser
            .lookup
            .binding_power_lookup
            .get(&token.token_type)
            .unwrap_or(&BindingPower::None);

        if binding_power > token_binding_power {
            break;
        }

        let left_expression_handler = parser.lookup.left_expression_lookup.get(&token.token_type);

        if left_expression_handler.is_none() {
            break;
        }

        let left_expression_handler = left_expression_handler.unwrap();

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
