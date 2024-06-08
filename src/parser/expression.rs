use crate::scanner::lexeme::{Lexeme, Token};

use super::{lookup::BindingPower, BinaryOperation, Expression, Parser};

pub fn parse(
    parser: &Parser,
    cursor: usize,
    binding_power: &BindingPower,
) -> Option<(Expression, usize)> {
    let mut cursor = cursor;

    // Consume the current lexeme
    let lexeme = parser.lexemes.get(cursor)?;
    let token = match lexeme {
        Lexeme::Valid(token, _) => token,
        Lexeme::Invalid(_) => return None,
    };

    let expression_handler = parser.lookup.expression_lookup.get(token)?;

    let (mut left_hand_side, new_cursor) = expression_handler(parser, cursor)?;
    cursor = new_cursor;

    while let Some(lexeme) = parser.lexemes.get(cursor) {
        let token = match lexeme {
            Lexeme::Valid(token, _) => token,
            Lexeme::Invalid(_) => break,
        };

        let token_binding_power = parser.lookup.binding_power_lookup.get(token)?;

        if binding_power > token_binding_power {
            break;
        }

        let left_expression_handler = parser.lookup.left_expression_lookup.get(token)?;
        let (right_hand_side, new_cursor) =
            left_expression_handler(parser, cursor, left_hand_side, binding_power)?;

        cursor = new_cursor;
        left_hand_side = right_hand_side;
    }

    Some((left_hand_side, cursor))
}

pub fn parse_binary(
    parser: &Parser,
    cursor: usize,
    left_hand_side: Box<Expression>,
    binding_power: &BindingPower,
) -> Option<(Expression, usize)> {
    let operator = parser.lexemes.get(cursor)?;
    let operator = match operator {
        Lexeme::Valid(Token::Plus, _) => BinaryOperation::Plus,
        Lexeme::Valid(Token::Minus, _) => BinaryOperation::Minus,
        Lexeme::Valid(Token::Star, _) => BinaryOperation::Times,
        Lexeme::Valid(Token::Slash, _) => BinaryOperation::Divide,
        _ => return None,
    };

    let (right_hand_side, cursor) = parse(parser, cursor, binding_power)?;

    Some((
        Expression::Binary(left_hand_side, operator, Box::new(right_hand_side)),
        cursor,
    ))
}
