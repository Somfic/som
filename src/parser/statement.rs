use crate::scanner::lexeme::{Lexeme, Range, Token};

use super::{expression, lookup::BindingPower, Diagnostic, Parser, Statement};

pub fn parse(parser: &Parser, cursor: usize) -> Result<(Statement, usize), Diagnostic> {
    let mut cursor = cursor;
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
    let lexeme = parser.lexemes.get(new_cursor);
    if let Some(Lexeme::Valid(Token::Semicolon, _)) = lexeme {
        cursor = new_cursor + 1;
    } else {
        return Err(Diagnostic::error(
            &Range {
                position: parser.lexemes.get(new_cursor - 1).unwrap().range().position + 1,
                length: 1,
            },
            "Expected semicolon",
        ));
    }

    Ok((Statement::Expression(expression), cursor))
}
