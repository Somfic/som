use crate::diagnostic::Diagnostic;

use super::{ast::Expression, lookup::BindingPower, macros::expect_token, ParseResult, Parser};
use std::collections::HashSet;

pub mod literals;

pub fn parse<'a>(
    parser: &mut Parser<'a>,
    binding_power: &BindingPower,
) -> ParseResult<'a, Expression> {
    let expression_handler = parser
        .lookup
        .expression_lookup
        .get(&parser.peek().unwrap().token_type)
        .ok_or(Diagnostic::error("P0001", "Expected a new expression"))?;

    let mut left_hand_side = expression_handler(parser)?;

    while parser.has_tokens() {
        let token = parser.peek().unwrap();

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

        left_hand_side = left_expression_handler(parser, left_hand_side, token_binding_power)?;
    }

    Ok(left_hand_side)
}

pub fn parse_addative<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Expression> {
    let plus = expect_token!(parser, Plus)?;
    let left = parse(parser, &BindingPower::Additive)?;

    todo!()
}

pub(crate) fn register(lookup: &mut super::lookup::Lookup) {
    literals::register(lookup);
}
