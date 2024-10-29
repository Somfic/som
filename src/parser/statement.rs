use crate::lexer::TokenKind;

use super::{ast::Statement, expression, lookup::BindingPower, Parser};
use miette::{Context, Result};

pub fn parse<'de>(parser: &mut Parser<'de>, optional_semicolon: bool) -> Result<Statement<'de>> {
    let expression = expression::parse(parser, BindingPower::None)?;

    Ok(Statement::Expression(expression))

    // let token = match parser.lexer.peek().as_ref() {
    //     Some(Ok(token)) => token,
    //     Some(Err(err)) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
    //     None => {
    //         return Err(miette::miette! {
    //             help = "expected a statement",
    //             "expected a statement"
    //         }
    //         .with_source_code(parser.source.to_string()))
    //     }
    // };

    // todo!()

    // let statement_handler = parser.lookup.statement_lookup.get(&token.kind);

    // // let expression = expression::parse(parser, BindingPower::None)?;

    // // if !optional_semicolon {
    // //     parser.lexer.expect(
    // //         TokenKind::Semicolon,
    // //         "expected a semicolon at the end of an expression",
    // //     )?;
    // // }

    // Ok(Statement::Expression(expression))
}
