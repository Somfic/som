use crate::scanner::lexeme::{Lexeme, Range, TokenType};

use super::{
    expression, lookup::BindingPower, macros::expect_token, Diagnostic, Parser, Statement,
};

pub fn parse(parser: &Parser, cursor: usize) -> Result<(Statement, usize), Diagnostic> {
    let lexeme = parser.lexemes.get(cursor);

    if lexeme.is_none() {
        return Err(Diagnostic::error(
            &Range {
                position: cursor,
                length: 0,
            },
            "Expected expression",
        ));
    }

    let lexeme = lexeme.unwrap();

    if let Lexeme::Invalid(_) = lexeme {
        return Err(Diagnostic::error(lexeme.range(), "Invalid token"));
    }

    let (expression, new_cursor) = expression::parse(parser, cursor, &BindingPower::None)?;

    // Expect a semicolon
    let (_, cursor) = expect_token!(parser, new_cursor, TokenType::Semicolon)?;

    Ok((Statement::Expression(expression), cursor))
}
