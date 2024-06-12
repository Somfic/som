use core::panic;

use crate::scanner::lexeme::{Lexeme, Range, TokenType, TokenValue};

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

    let token = match lexeme {
        Lexeme::Valid(token) => token,
        Lexeme::Invalid(_) => return Err(Diagnostic::error(lexeme.range(), "Invalid token")),
    };

    let statement_handler = parser.lookup.statement_lookup.get(&token.token_type);

    match statement_handler {
        Some(statement_handler) => statement_handler(parser, cursor),
        None => parse_expression(parser, cursor),
    }
}

pub fn parse_expression(parser: &Parser, cursor: usize) -> Result<(Statement, usize), Diagnostic> {
    let (expression, cursor) = expression::parse(parser, cursor, &BindingPower::None)?;
    let (_, cursor) = expect_token!(parser, cursor, TokenType::Semicolon)?;

    Ok((Statement::Expression(expression), cursor))
}

pub fn parse_declaration(parser: &Parser, cursor: usize) -> Result<(Statement, usize), Diagnostic> {
    let (_, cursor) = expect_token!(parser, cursor, TokenType::Var)?;
    let (identifier, cursor) = expect_token!(parser, cursor, TokenType::Identifier)?;
    let identifier = match &identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => panic!("expect_token! should return a valid token and handle the error case"),
    };
    let (_, cursor) = expect_token!(parser, cursor, TokenType::Equal)?;
    let (expression, cursor) = expression::parse(parser, cursor, &BindingPower::None)?;
    let (_, cursor) = expect_token!(parser, cursor, TokenType::Semicolon)?;

    Ok((
        Statement::Declaration(identifier.clone(), expression),
        cursor,
    ))
}
