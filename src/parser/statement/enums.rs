use crate::{
    parser::{ast::Statement, lookup::Lookup, macros::expect_token, ParseResult, Parser},
    scanner::lexeme::TokenType,
};

pub fn register(lookup: &mut Lookup) {
    lookup.add_statement_handler(TokenType::Enum, parse_enum);
}

fn parse_enum<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Enum)?; 

    todo!()
}
