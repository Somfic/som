use super::{BindingPower, Parser};
use crate::{
    ast::{
        combine_spans, CombineSpan, ExpressionValue, Identifier, Statement, StatementValue, Typing,
        TypingValue,
    },
    tokenizer::{Token, TokenKind},
    Result,
};

pub fn parse_block(parser: &mut Parser) -> Result<Statement> {
    parser
        .tokens
        .expect(TokenKind::CurlyOpen, "expected the start of a block")?;

    let mut statements = Vec::new();
    loop {
        if parser.tokens.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::CurlyClose)
        }) {
            break;
        }

        let statement = parser.parse_statement(true)?;
        statements.push(statement);
    }

    parser
        .tokens
        .expect(TokenKind::CurlyClose, "expected the end of a block")?;

    let span = combine_spans(statements.iter().map(|s| s.span).collect());

    Ok(StatementValue::Block(statements).with_span(span))
}

pub fn parse_declaration(parser: &mut Parser) -> Result<Statement> {
    parser
        .tokens
        .expect(TokenKind::Let, "expected a variable declaration")?;

    let identifier = parser
        .tokens
        .expect(TokenKind::Identifier, "expected the variable name")?;

    let identifier = Identifier::from_token(&identifier)?;

    let explicit_type = if parser.tokens.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::Tilde)
    }) {
        parser
            .tokens
            .expect(TokenKind::Tilde, "expected an explicit type")?;

        Some(parser.parse_typing(BindingPower::None)?)
    } else {
        None
    };

    parser
        .tokens
        .expect(TokenKind::Equal, "expected an equals sign")?;

    let value = parser.parse_expression(BindingPower::None)?;

    let span = identifier.span.combine(value.span);

    Ok(StatementValue::Declaration {
        identifier,
        explicit_type,
        value,
    }
    .with_span(span))
}

pub fn parse_condition(parser: &mut Parser) -> Result<Statement> {
    parser
        .tokens
        .expect(TokenKind::If, "expected an if statement")?;

    let condition = parser.parse_expression(BindingPower::None)?;
    let body = parser.parse_statement(false)?;

    let span = condition.span.combine(body.span);

    Ok(StatementValue::Condition(condition, Box::new(body)).with_span(span))
}

pub fn parse_while_loop(parser: &mut Parser) -> Result<Statement> {
    let token = parser
        .tokens
        .expect(TokenKind::While, "expected a while statement")?;

    let condition = parser.parse_expression(BindingPower::None)?;
    let body = parser.parse_statement(false)?;

    let span = token.span.combine(condition.span);

    Ok(StatementValue::WhileLoop(condition, Box::new(body)).with_span(span))
}
