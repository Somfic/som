use crate::{
    parser::{
        ast::Statement,
        lookup::{BindingPower, Lookup},
        macros::{expect_token, expect_value, optional_token},
        typest, ParseResult, Parser,
    },
    scanner::lexeme::TokenType,
};
use std::collections::HashMap;

pub fn register(lookup: &mut Lookup) {
    lookup.add_statement_handler(TokenType::Struct, parse_struct);
}

pub fn parse_struct<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Struct)?;
    let identifier = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier, Identifier);
    expect_token!(parser, Colon)?;

    let indentation = optional_token!(parser, IndentationOpen);

    let mut members = HashMap::new();

    loop {
        let token = expect_token!(parser)?;

        if token.token_type == TokenType::Semicolon {
            break;
        }

        if indentation.is_some() && token.token_type == TokenType::IndentationClose {
            break;
        }

        let member = expect_token!(parser, Identifier)?;
        let member_name = expect_value!(member, Identifier);

        expect_token!(parser, Tilde)?;

        let typest = typest::parse(parser, BindingPower::None)?;

        members.insert(member_name, typest); // TODO: Error if member already exists
    }

    if indentation.is_some() {
        optional_token!(parser, Semicolon);
        expect_token!(parser, IndentationClose)?;
    } else {
        expect_token!(parser, Semicolon)?;
    }

    Ok(Statement::Struct(identifier, members))
}
