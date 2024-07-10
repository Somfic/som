use std::collections::{HashMap, HashSet};

use crate::{
    parser::{
        ast::Statement,
        lookup::{BindingPower, Lookup},
        macros::{expect_token, expect_value, optional_token, warn_unneeded_token},
        typest, ParseResult, Parser,
    },
    scanner::lexeme::TokenType,
};

pub fn register(lookup: &mut Lookup) {
    lookup.add_statement_handler(TokenType::Enum, parse_enum);
}

fn parse_enum<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Enum)?;
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

        let typest = optional_token!(parser, Tilde)
            .map(|_| typest::parse(parser, BindingPower::None))
            .transpose()?;

        if let Some(_) = members.insert(member_name.clone(), typest) {
            warn_unneeded_token!(parser, member);
        }
    }

    if indentation.is_some() {
        optional_token!(parser, Semicolon);
        expect_token!(parser, IndentationClose)?;
    } else {
        expect_token!(parser, Semicolon)?;
    }

    Ok(Statement::Enum(identifier, members))
}
