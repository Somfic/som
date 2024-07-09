use super::{ast::Statement, expression, macros::expect_token, ParseResult, Parser};

pub fn parse<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    let statement_handler = parser
        .lookup
        .statement_lookup
        .get(&parser.peek().unwrap().token_type);

    match statement_handler {
        Some(handler) => handler(parser),
        None => parse_expression(parser),
    }
}

fn parse_expression<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    let expression = expression::parse(parser)?;
    expect_token!(parser, Semicolon)?;

    Ok(Statement::Expression(expression))
}
