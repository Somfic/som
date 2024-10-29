use crate::lexer::TokenKind;

use super::{ast::Statement, expression, lookup::BindingPower, Parser};
use miette::{Context, Result};

pub fn parse<'de>(parser: &mut Parser<'de>, optional_semicolon: bool) -> Result<Statement<'de>> {
    let expression = expression::parse(parser, BindingPower::None)?;

    if !optional_semicolon {
        parser.lexer.expect(
            TokenKind::Semicolon,
            "expected a semicolon at the end of an expression",
        )?;
    }

    Ok(Statement::Expression(expression))
}
