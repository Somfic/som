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


pub fn parse_function<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Statement<'ast>> {
    parser
        .tokens
        .expect(TokenKind::Function, "expected a function declaration")?;

    let identifier = parser
        .tokens
        .expect(TokenKind::Identifier, "expected the function name")?;

    let identifier_name = match identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    parser
        .tokens
        .expect(TokenKind::LeftParen, "expected a left parenthesis")?;

    let mut parameters = vec![];
    while parser.tokens.peek().is_some() {
        let token = parser.tokens.peek().unwrap()?;
        match token.kind {
            TokenKind::RightParen => break,
            TokenKind::Identifier => {
                let parameter = parser.tokens.next().unwrap()?;
                let parameter_name = match parameter.value {
                    TokenValue::Identifier(identifier) => identifier,
                    _ => unreachable!(),
                };
                parameters.push(parameter_name);
            }
            _ => return Err(parser.error("expected a right parenthesis or an identifier"))?,
        }
    }

    parser
        .tokens
        .expect(TokenKind::RightParen, "expected a right parenthesis")?;

    parser
        .tokens
        .expect(TokenKind::LeftBrace, "expected a left brace")?;

    let mut statements = vec![];
    while parser.tokens.peek().is_some() {
        let token = parser.tokens.peek().unwrap()?;
        match token.kind {
            TokenKind::RightBrace => break,
            _ => {
                let statement = parser.parse_statement(false)?;
                statements.push(statement);
            }
        }
    }
