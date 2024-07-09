use super::{
    ast::Statement, expression, lookup::BindingPower, macros::expect_token, ParseResult, Parser,
};

pub mod enums;

pub fn parse<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    let statement_handler = parser
        .lookup
        .statement_lookup
        .get(&parser.peek().unwrap().token_type);

    match statement_handler {
        Some(handler) => {
            println!("Using statement handler");
            handler(parser)
        }
        None => parse_expression(parser),
    }
}

pub fn register(lookup: &mut super::lookup::Lookup) {
    enums::register(lookup);
}

fn parse_expression<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Statement> {
    println!("Parsing expression");

    let expression = expression::parse(parser, BindingPower::None)?;
    expect_token!(parser, Semicolon)?;

    Ok(Statement::Expression(expression))
}
