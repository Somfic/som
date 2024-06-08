use crate::scanner::lexeme::{Lexeme, Token};

use super::{expression, lookup::BindingPower, Parser, Statement};

pub fn parse(parser: &Parser, cursor: usize) -> Option<(Statement, usize)> {
    let mut cursor = cursor;
    let lexeme = parser.lexemes.get(cursor)?;
    if let Lexeme::Invalid(_) = lexeme {
        return None;
    }

    let (expression, new_cursor) = expression::parse(parser, cursor, &BindingPower::None)?;

    // Expect a semicolon
    let lexeme = parser.lexemes.get(new_cursor);
    if let Some(Lexeme::Valid(Token::Semicolon, _)) = lexeme {
        cursor = new_cursor + 1;
    } else {
        return None;
    }

    Some((Statement::Expression(expression), cursor))
}
