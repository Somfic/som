use crate::lexer::{TokenKind, TokenValue};

use super::{ast::Statement, expression, lookup::BindingPower, Parser};
use miette::{Context, Result};

pub fn parse<'de>(parser: &mut Parser<'de>, optional_semicolon: bool) -> Result<Statement<'de>> {
    let token = match parser.lexer.peek().as_ref() {
        Some(Ok(token)) => token,
        Some(Err(err)) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
        None => {
            return Err(miette::miette! {
                help = "expected a statement",
                "expected a statement"
            })
        }
    };

    let statement_handler = parser.lookup.statement_lookup.get(&token.kind);

    let statement = match statement_handler {
        Some(handler) => handler(parser)?,
        None => Statement::Expression(expression::parse(parser, BindingPower::None)?),
    };

    if !optional_semicolon {
        parser.lexer.expect(
            TokenKind::Semicolon,
            "expected a semicolon at the end of an expression",
        )?;
    };

    Ok(statement)
}

pub fn let_<'de>(parser: &mut Parser<'de>) -> Result<Statement<'de>> {
    parser
        .lexer
        .expect(TokenKind::Let, "expected a let keyword")?;
    let identifier = parser
        .lexer
        .expect(TokenKind::Identifier, "expected an identifier")?;
    let identifier = match identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };
    parser
        .lexer
        .expect(TokenKind::Equal, "expected an equal sign")?;
    let expression = expression::parse(parser, BindingPower::None)?;

    Ok(Statement::Assignment(identifier, expression))
}
