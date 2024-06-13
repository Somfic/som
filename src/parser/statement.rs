use crate::scanner::lexeme::{Lexeme, TokenType, TokenValue};
use core::panic;
use std::collections::{HashMap, HashSet};

use super::{
    ast::Type,
    expression,
    lookup::BindingPower,
    macros::{
        expect_any_token, expect_expression, expect_token, expect_token_value, expect_tokens,
        expect_type, expect_valid_token,
    },
    Diagnostic, Parser, Statement,
};

pub fn parse(parser: &Parser, cursor: usize) -> Result<(Statement, usize), Diagnostic> {
    let (token, _) = expect_valid_token!(parser, cursor);
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
    let (_, cursor) = expect_token!(parser, cursor, TokenType::Let)?;
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
    let (tokens, cursor) = expect_tokens!(
        parser,
        cursor,
        TokenType::Struct,
        TokenType::Identifier,
        TokenType::Colon
    )?;

    let identifier = expect_token_value!(tokens[1], TokenValue::Identifier);

    let mut new_cursor = cursor;
    let mut members: HashMap<String, Type> = HashMap::new();

    while let Some(Lexeme::Valid(token)) = parser.lexemes.get(new_cursor) {
        let (member_name, member_type, cursor) = match token.token_type {
            TokenType::Semicolon => break,
            _ => {
                let (tokens, cursor) = expect_tokens!(
                    parser,
                    new_cursor,
                    TokenType::Dot,
                    TokenType::Identifier,
                    TokenType::Colon
                )?;

                let identifier = expect_token_value!(tokens[1], TokenValue::Identifier);

                let (field_type, cursor) = expect_type!(parser, cursor, BindingPower::None)?;

                (identifier, field_type, cursor)
            }
        };

        // TODO: Handle warning for overwritten member
        members.insert(member_name, member_type);

        new_cursor = cursor;
    }

    let (_, cursor) = expect_token!(parser, new_cursor, TokenType::Semicolon)?;

    Ok((Statement::Struct(identifier, members), cursor))
}

pub fn parse_enum(parser: &Parser, cursor: usize) -> Result<(Statement, usize), Diagnostic> {
    let (tokens, cursor) = expect_tokens!(
        parser,
        cursor,
        TokenType::Enum,
        TokenType::Identifier,
        TokenType::Colon
    )?;

    let identifier = expect_token_value!(tokens[1], TokenValue::Identifier);

    let mut new_cursor = cursor;
    let mut members: HashSet<String> = HashSet::new();

    while let Some(Lexeme::Valid(token)) = parser.lexemes.get(new_cursor) {
        let (member_name, cursor) = match token.token_type {
            TokenType::Semicolon => break,
            _ => {
                let (field_name, cursor) =
                    expect_token!(parser, new_cursor, TokenType::Identifier)?;

                let field_name = expect_token_value!(field_name, TokenValue::Identifier);

                (field_name, cursor)
            }
        };

        new_cursor = cursor;
        // TODO: Handle warning for overwritten members
        members.insert(member_name);
    }

    let (_, cursor) = expect_token!(parser, new_cursor, TokenType::Semicolon)?;

    Ok((Statement::Enum(identifier, members), cursor))
}
