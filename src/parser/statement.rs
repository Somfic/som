use crate::{
    ast::{Spannable, Statement, StatementValue},
    tokenizer::{TokenKind, TokenValue},
    ParserResult,
};

use super::{BindingPower, Parser};

pub fn parse_let<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Statement<'ast>> {
    parser
        .tokens
        .expect(TokenKind::Let, "expected a variable declaration")?;

    let identifier = parser
        .tokens
        .expect(TokenKind::Identifier, "expected the variable name")?;

    let identifier_name = match identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    parser
        .tokens
        .expect(TokenKind::Equal, "expected an equals sign")?;

    let expression = parser.parse_expression(BindingPower::Assignment)?;

    Ok(Statement::at_multiple(
        vec![identifier.span, expression.span],
        StatementValue::Declaration(identifier_name, expression),
    ))
}
