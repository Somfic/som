use super::{lookup::BindingPower, Expression, Parser};
use crate::scanner::lexeme::Lexeme;

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

        let token_binding_power = parser
            .lookup
            .binding_power_lookup
            .get(token)
            .unwrap_or(&BindingPower::None);

        if binding_power > token_binding_power {
            break;
        }

        let left_expression_handler = parser.lookup.left_expression_lookup.get(token);

        if left_expression_handler.is_none() {
            break;
        }

        let left_expression_handler = left_expression_handler.unwrap();

        let (right_hand_side, new_cursor) =
            left_expression_handler(parser, cursor, left_hand_side, binding_power)?;

        cursor = new_cursor;
        left_hand_side = right_hand_side;
    }

    Some((left_hand_side, cursor))
}
