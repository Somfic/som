use crate::{
    parser::{
        ast::{FieldSignature, Statement},
        lookup::{BindingPower, Lookup},
        macros::{expect_token, expect_value, optional_token},
        typest, ParseResult, Parser,
    },
    scanner::lexeme::TokenType,
};
use std::collections::{HashMap, HashSet};

use super::variables;

pub fn register(lookup: &mut Lookup) {
    lookup.add_statement_handler(TokenType::Struct, parse_struct);
}

pub fn parse_struct<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Struct)?;
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

        let field = parse_field(parser)?;

        members.insert(field); // TODO: Error if member already exists
    }

    if indentation.is_some() {
        optional_token!(parser, Semicolon);
        expect_token!(parser, IndentationClose)?;
    } else {
        expect_token!(parser, Semicolon)?;
    }

    Ok(Statement::Struct(identifier, members))
}

pub fn parse_field<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, FieldSignature> {
    let (name, typest) = variables::parse_explicit_variable_signature(parser, "field")?;

    Ok(FieldSignature { name, typest })
}
