use crate::lexer::TokenKind;

use super::{ast::Statement, expression, lookup::BindingPower, Parser};
use miette::{Context, Result};

pub fn parse<'de>(parser: &mut Parser<'de>) -> Result<Statement<'de>> {
    let expression = expression::parse(parser, BindingPower::None)?;
    parser.lexer.expect(
        TokenKind::Semicolon,
        "expected a semicolon at the end of an expression",
    )?;

    Ok(Statement::Expression(expression))
}
