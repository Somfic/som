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

pub fn parse<'de>(
    parser: &mut Parser<'de>,
    binding_power: BindingPower,
) -> Result<Expression<'de>> {
    let token = match parser.lexer.peek().as_ref() {
        Some(Ok(token)) => token,
        Some(Err(err)) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
        None => {
            return Err(miette::miette! {
                help = "expected a new expression",
                "expected a new expression, found EOF"
            }
            .with_source_code(parser.source.to_string()))
        }
    };

    let handler = parser.lookup.expression_lookup.get(&token.kind).ok_or(
        miette::miette! {
            labels = vec![token.label("expected a new expression")],
            help = format!("cannot parse {} into a new expression", token.kind),
            "expected a new expression, found {}", token.kind
        }
        .with_source_code(parser.source.to_string()),
    )?;

    let mut lhs = handler(parser)?;

    while let Some(token) = parser.lexer.peek() {
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
    }

    Ok(lhs)
}

pub fn add_handlers<'de>(lookup: &'de mut Lookup<'de>) {
    lookup.add_left_expression_handler(
        TokenKind::Plus,
        BindingPower::Additive,
        |parser: &mut Parser<'_>, lhs: Expression<'_>, bp| -> Result<Expression<'_>> {
            let rhs = parse(parser, bp)?;
            Ok(Expression::Binary {
                operator: BinaryOperator::Add,
                left: Box::new(lhs),
                right: Box::new(rhs),
            })
        },
    );
}
