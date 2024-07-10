use std::collections::HashMap;

use crate::{
    parser::{
        ast::{Statement, Type},
        lookup::{BindingPower, Lookup},
        macros::{expect_token, expect_value, optional_token},
        typest, ParseResult, Parser,
    },
    scanner::lexeme::TokenType,
};

pub fn register(lookup: &mut Lookup) {
    lookup.add_statement_handler(TokenType::Function, parse_function);
    lookup.add_statement_handler(TokenType::LeftArrow, parse_return);
}

fn parse_function<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Function)?;
    let identifier = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier, Identifier);

    expect_token!(parser, ParenOpen)?;

    let mut parameters = HashMap::new();

    loop {
        let token = expect_token!(parser)?;

        if token.token_type == TokenType::ParenClose {
            break;
        }

        let parameter = expect_token!(parser, Identifier)?;
        let parameter = expect_value!(parameter, Identifier);

        expect_token!(parser, Tilde)?;

        let typest = typest::parse(parser, BindingPower::None)?;

        parameters.insert(parameter, typest);

        optional_token!(parser, Comma);
    }

    expect_token!(parser, ParenClose)?;

    let return_type = optional_token!(parser, RightArrow)
        .map(|_| typest::parse(parser, BindingPower::None))
        .transpose()?
        .unwrap_or(Type::Void);

    expect_token!(parser, Colon)?;

    let body = crate::parser::statement::parse(parser)?;

    Ok(Statement::Function(
        identifier,
        parameters,
        return_type,
        Box::new(body),
    ))
}

fn parse_return<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, LeftArrow)?;

    let expression = crate::parser::expression::parse(parser, BindingPower::None)?;

    Ok(Statement::Return(expression))
}
