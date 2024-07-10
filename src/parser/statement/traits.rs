use std::collections::HashSet;

use crate::{
    parser::{
        ast::{Function, Statement},
        lookup::{BindingPower, Lookup},
        macros::{expect_token, expect_value},
        ParseResult, Parser,
    },
    scanner::lexeme::TokenType,
};

pub fn register(lookup: &mut Lookup) {
    lookup.add_statement_handler(TokenType::Trait, parse_trait);
    lookup.add_statement_handler(TokenType::Implementation, parse_impl);
}

fn parse_trait<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Trait)?;
    let identifier = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier, Identifier);

    expect_token!(parser, Colon)?;

    expect_token!(parser, IndentationOpen)?;

    let mut functions = HashSet::new();

    loop {
        let token = expect_token!(parser)?;

        if token.token_type == TokenType::IndentationClose {
            break;
        }

        let function = crate::parser::statement::functions::parse_function_signature(parser)?;
        functions.insert(function); // TODO: Error if function already exists
    }

    expect_token!(parser, IndentationClose)?;

    Ok(Statement::Trait(identifier, functions))
}

fn parse_impl<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Implementation)?;
    let identifier = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier, Identifier);

    expect_token!(parser, For)?;

    let typest = crate::parser::typest::parse(parser, BindingPower::None)?;

    expect_token!(parser, Colon)?;
    expect_token!(parser, IndentationOpen)?;

    let mut functions = HashSet::new();

    loop {
        let token = expect_token!(parser)?;

        if token.token_type == TokenType::IndentationClose {
            break;
        }

        let signature = crate::parser::statement::functions::parse_function_signature(parser)?;
        expect_token!(parser, Colon)?;
        let body = crate::parser::statement::parse(parser)?;

        functions.insert(Function {
            signature,
            body: Box::new(body),
        }); // TODO: Error if function already exists
    }

    expect_token!(parser, IndentationClose)?;

    Ok(Statement::Implementation(identifier, typest, functions))
}
