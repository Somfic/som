use miette::Result;

use super::{ast::Expression, Parser};

pub fn parse<'de>(parser: &mut Parser<'de>) -> Result<Expression<'de>> {
    let token = parser.lexer.expect_any("expected an expression")?;
    let handler = parser.lookup.expression_lookup.get(&token.kind).ok_or(
        miette::miette! {
            labels = vec![token.label("expected a new expression")],
            help = format!("cannot parse {} into a new expression", token.kind),
            "expected a new expression, found {}", token.kind
        }
        .with_source_code(parser.source.to_string()),
    )?;

    let mut lhs = handler(parser)?;

    loop {
        let peeked = match parser.lexer.peek().cloned() {
            Some(token) => token,
            None => break,
        }?;

        let bp = parser.lookup.binding_power_lookup.get(&peeked.kind);
    }

    todo!()
}
