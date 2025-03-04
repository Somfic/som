use crate::{
    ast::{Spannable, Statement, StatementValue},
    tokenizer::{TokenKind, TokenValue},
    ParserResult,
};

use super::{BindingPower, Parser};

pub fn parse_block<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Statement<'ast>> {
    parser
        .tokens
        .expect(TokenKind::CurlyOpen, "expected the start of a block")?;

    let mut statements = Vec::new();
    loop {
        if parser
            .tokens
            .peek()
            .is_some_and(|token| token.kind == TokenKind::CurlyClose)
        {
            break;
        }

        let statement = parser.parse_statement(true)?;
        statements.push(statement);
    }

    parser
        .tokens
        .expect(TokenKind::CurlyClose, "expected the end of a block")?;

    Ok(Statement::at_multiple(
        statements.iter().map(|s| s.span).collect(),
        StatementValue::Block(statements),
    ))
}

pub fn parse_declaration<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Statement<'ast>> {
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

pub fn parse_condition<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Statement<'ast>> {
    parser
        .tokens
        .expect(TokenKind::If, "expected an if statement")?;

    let condition = parser.parse_expression(BindingPower::None)?;
    let body = parser.parse_statement(false)?;

    Ok(Statement::at_multiple(
        vec![condition.span, body.span],
        StatementValue::Condition(condition, Box::new(body)),
    ))
}
