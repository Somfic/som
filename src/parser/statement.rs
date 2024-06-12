use core::panic;
use std::collections::HashMap;

use crate::{
    parser::macros::expect_optional_token,
    scanner::lexeme::{Lexeme, Range, TokenType, TokenValue},
};

use super::{
    ast::Type,
    expression,
    lookup::BindingPower,
    macros::{expect_any_token, expect_expression, expect_token, expect_type},
    Diagnostic, Parser, Statement,
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

    let (token, _) = expect_any_token!(parser, cursor, TokenType::Colon, TokenType::Equal)?;
    let (typing, cursor) = match token.token_type {
        TokenType::Colon => {
            let (_, cursor) = expect_token!(parser, cursor, TokenType::Colon)?;
            let (typing, cursor) = expect_type!(parser, cursor, BindingPower::None)?;
            (Some(typing), cursor)
        }
        _ => (None, cursor),
    };

    let (_, cursor) = expect_token!(parser, cursor, TokenType::Equal)?;
    let (expression, cursor) = expect_expression!(parser, cursor, &BindingPower::None)?;
    let (_, cursor) = expect_token!(parser, cursor, TokenType::Semicolon)?;

    Ok((
        Statement::Declaration(identifier.clone(), typing, expression),
        cursor,
    ))
}

pub fn parse_struct(parser: &Parser, cursor: usize) -> Result<(Statement, usize), Diagnostic> {
    let (_, cursor) = expect_token!(parser, cursor, TokenType::Struct)?;
    let (name, cursor) = expect_token!(parser, cursor, TokenType::Identifier)?;
    let name = match &name.value {
        TokenValue::Identifier(name) => name.clone(),
        _ => panic!("expect_token! should return a valid token and handle the error case"),
    };

    let (_, cursor) = expect_token!(parser, cursor, TokenType::CurlyOpen)?;
    let mut new_cursor = cursor;
    let mut members: HashMap<String, Type> = HashMap::new();

    while let Some(Lexeme::Valid(token)) = parser.lexemes.get(new_cursor) {
        let (member_name, member_type, cursor) = match token.token_type {
            TokenType::CurlyClose => break,
            _ => {
                if !members.is_empty() {
                    let (_, cursor) = expect_token!(parser, new_cursor, TokenType::Comma)?;
                    new_cursor = cursor;
                }

                let (field_name, cursor) =
                    expect_token!(parser, new_cursor, TokenType::Identifier)?;
                let field_name = match &field_name.value {
                    TokenValue::Identifier(field_name) => field_name.clone(),
                    _ => panic!(
                        "expect_token! should return a valid token and handle the error case"
                    ),
                };

                let (_, cursor) = expect_token!(parser, cursor, TokenType::Colon)?;
                let (field_type, cursor) = expect_type!(parser, cursor, BindingPower::None)?;

                (field_name, field_type, cursor)
            }
        };

        // TODO: Handle warning for overwritten member  
        members.insert(member_name, member_type);

        new_cursor = cursor;
    }

    let (_, cursor) = expect_token!(parser, new_cursor, TokenType::CurlyClose)?;

    Ok((Statement::Struct(name, members), cursor))
}
