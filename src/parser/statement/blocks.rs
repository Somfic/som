use crate::{
    parser::{ast::Statement, lookup::Lookup, macros::expect_token, ParseResult, Parser},
    scanner::lexeme::TokenType,
};

pub fn register(lookup: &mut Lookup) {
    lookup.add_statement_handler(TokenType::IndentationOpen, parse_block);
}

fn parse_block<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    let mut statements = Vec::new();

    expect_token!(parser, IndentationOpen)?;

    loop {
        let token = expect_token!(parser)?;

        if token.token_type == TokenType::IndentationClose {
            break;
        }

        let statement = crate::parser::statement::parse(parser)?;

        statements.push(statement);
    }

    expect_token!(parser, IndentationClose)?;

    Ok(Statement::Block(statements))
}
