use std::collections::HashSet;

use crate::{
    parser::{
        ast::{FieldSignature, Function, Statement},
        lookup::{BindingPower, Lookup},
        macros::{either_token, expect_token, expect_value},
        ParseResult, Parser,
    },
    scanner::lexeme::TokenType,
};

use super::{functions, structs};

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
    let mut fields = HashSet::new();

    loop {
        let token = expect_token!(parser)?;

        if token.token_type == TokenType::IndentationClose {
            break;
        }

        let token = either_token!(parser, Function, Identifier)?;

        match token.token_type {
            TokenType::Function => {
                let function = functions::parse_function_signature(parser)?;
                functions.insert(function); // TODO: Warning on duplicate
            }
            TokenType::Identifier => {
                let field = structs::parse_field(parser)?;
                fields.insert(field); // TODO: Warning on duplicate
            }
            _ => unreachable!(),
        }

        // TODO: Error if function already exists
    }

    expect_token!(parser, IndentationClose)?;

    Ok(Statement::Trait(identifier, functions, fields))
}

fn parse_impl<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Implementation)?;
    let identifier = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier, Identifier);

    expect_token!(parser, For)?;

    let typest = expect_token!(parser, Identifier)?;
    let typest = expect_value!(typest, Identifier);

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
