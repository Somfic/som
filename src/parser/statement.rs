use crate::scanner::lexeme::Lexeme;

use super::{expression, lookup::BindingPower, Parser, Statement};

pub fn parse(parser: &Parser, cursor: usize) -> Option<(Statement, usize)> {
    let lexeme = parser.lexemes.get(cursor)?;
    let token = match lexeme {
        Lexeme::Valid(token, _) => token,
        Lexeme::Invalid(_) => return None,
    };

    let (expression, cursor) = expression::parse(parser, cursor, &BindingPower::None)?;

    Some((Statement::Expression(expression), cursor))
}
