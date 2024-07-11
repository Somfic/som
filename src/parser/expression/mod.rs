use super::{ast::Expression, lookup::BindingPower, macros::expect_token, ParseResult, Parser};
use crate::diagnostic::{Diagnostic, Snippet};

pub mod binary;
pub mod functions;
pub mod literals;
pub mod structs;

pub fn parse<'a>(
    parser: &mut Parser<'a>,
    binding_power: BindingPower,
) -> ParseResult<'a, Expression> {
    let token = expect_token!(parser)?;

    let expression_handler = parser
        .lookup
        .expression_lookup
        .get(&token.token_type)
        .ok_or(
            Diagnostic::error("P0001", "Expected a new expression").with_snippet(
                Snippet::primary_from_token(token, "Expected an expression to start here"),
            ),
        )?;

    let mut left_hand_side = expression_handler(parser)?;

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

        let left_expression_handler =
            match parser.lookup.left_expression_lookup.get(&token.token_type) {
                Some(handler) => handler,
                None => break,
            };

        left_hand_side = left_expression_handler(parser, left_hand_side, token_binding_power)?;
    }

    Ok(left_hand_side)
}

pub(crate) fn register(lookup: &mut super::lookup::Lookup) {
    literals::register(lookup);
    binary::register(lookup);
    structs::register(lookup);
    functions::register(lookup);

    lookup.add_expression_handler(crate::scanner::lexeme::TokenType::ParenOpen, parse_grouping);
}

fn parse_grouping<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Expression> {
    expect_token!(parser, ParenOpen)?;
    let expression = parse(parser, BindingPower::None)?;
    expect_token!(parser, ParenClose)?;

    Ok(Expression::Grouping(Box::new(expression)))
}
