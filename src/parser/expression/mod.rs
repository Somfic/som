use miette::Result;

use crate::{
    lexer::{TokenKind, TokenValue},
    parser::lookup::BindingPower,
};

use super::{
    ast::{BinaryOperator, Expression, Primitive},
    lookup::Lookup,
    Parser,
};

pub mod binary;
pub mod primitive;

pub fn parse<'de>(
    parser: &mut Parser<'de>,
    binding_power: BindingPower,
) -> Result<Expression<'de>> {
    let token = match parser.lexer.peek().as_ref() {
        Some(Ok(token)) => token,
        Some(Err(err)) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
        None => {
            return Err(miette::miette! {
                help = "expected an expression",
                "expected an expression"
            }
            .with_source_code(parser.source.to_string()))
        }
    };

    let handler = parser.lookup.expression_lookup.get(&token.kind).ok_or(
        miette::miette! {
            labels = vec![token.label("expected an expression")],
            help = format!("{} is not an expression", token.kind),
            "expected an expression, found {}", token.kind
        }
        .with_source_code(parser.source.to_string()),
    )?;
    let mut lhs = handler(parser)?;

    let mut next_token = parser.lexer.peek();

    while let Some(token) = next_token {
        let token = match token {
            Ok(token) => token,
            Err(err) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
        };

        let token_binding_power = {
            let binding_power_lookup = parser.lookup.binding_power_lookup.clone();
            binding_power_lookup
                .get(&token.kind)
                .unwrap_or(&BindingPower::None)
                .clone()
        };

        if binding_power > token_binding_power {
            break;
        }

        let handler = match parser.lookup.left_expression_lookup.get(&token.kind) {
            Some(handler) => handler,
            None => break,
        };

        parser.lexer.next();

        lhs = handler(parser, lhs, token_binding_power)?;

        next_token = parser.lexer.peek();
    }

    Ok(lhs)
}
