use std::collections::{HashMap, HashSet};

use crate::{
    parser::{
        ast::{EnumMember, Statement},
        lookup::{BindingPower, Lookup},
        macros::{expect_token, expect_value, optional_token, warn_unneeded_token},
        typest, ParseResult, Parser,
    },
    scanner::lexeme::TokenType,
};

use super::variables;

pub fn register(lookup: &mut Lookup) {
    lookup.add_statement_handler(TokenType::Enum, parse_enum);
}

fn parse_enum<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Enum)?;
    let identifier = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier, Identifier);
    expect_token!(parser, Colon)?;

    let indentation = optional_token!(parser, IndentationOpen);

    let mut members = HashSet::new();

    loop {
        let token = expect_token!(parser)?;

        if token.token_type == TokenType::Semicolon {
            break;
        }

        if indentation.is_some() && token.token_type == TokenType::IndentationClose {
            break;
        }

        let enum_member = parse_enum_member(parser)?;

        members.insert(enum_member); // TODO: Error if member already exists
    }

    if indentation.is_some() {
        optional_token!(parser, Semicolon);
        expect_token!(parser, IndentationClose)?;
    } else {
        expect_token!(parser, Semicolon)?;
    }

    Ok(Statement::Enum(identifier, members))
}

pub fn parse_enum_member<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, EnumMember> {
    let (name, typest) = variables::parse_variable_signature(parser)?;

    Ok(EnumMember { name, typest })
}
