use std::borrow::Cow;

use crate::{
    ast::{Spannable, Statement, StatementValue, StructDeclaration, StructMember},
    tokenizer::{Token, TokenKind, TokenValue},
    ParserResult,
};

use super::{module, BindingPower, Parser};

pub fn parse_block<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Statement<'ast>> {
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

    let identifier_name = match identifier.value.clone() {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    parser
        .tokens
        .expect(TokenKind::Equal, "expected an equals sign")?;

    let declaration = match parser.tokens.peek() {
        Some(Ok(token)) => match token.kind {
            TokenKind::Function => parse_function_declaration(parser, identifier),
            TokenKind::Intrinsic => parse_intrinsic_declaration(parser, identifier),
            TokenKind::Type => parse_type_declaration(parser, identifier, identifier_name),
            _ => parse_variable_declaration(parser, identifier, identifier_name),
        },
        _ => unreachable!(),
    }?;

    Ok(declaration)
}

fn parse_function_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
) -> ParserResult<Statement<'ast>> {
    let function = module::parse_function(parser, identifier.clone())?;

    Ok(Statement::at_multiple(
        vec![identifier.span, function.span],
        StatementValue::Function(function),
    ))
}

fn parse_type_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
    identifier_name: Cow<'ast, str>,
) -> ParserResult<Statement<'ast>> {
    match parser.tokens.peek() {
        Some(Ok(token)) => match token.kind {
            TokenKind::CurlyOpen => parse_struct_declaration(parser, identifier),
            _ => todo!(),
        },
        _ => unreachable!(),
    }
}

fn parse_struct_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
) -> ParserResult<Statement<'ast>> {
    parser
        .tokens
        .expect(TokenKind::CurlyOpen, "expected a struct")?;

    let identifier_name = match identifier.value.clone() {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    let mut members = vec![];

    while parser.tokens.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind != TokenKind::CurlyClose)
    }) {
        let identifier = parser
            .tokens
            .expect(TokenKind::Identifier, "expected a struct member")?;
        let identifier_name = match identifier.value.clone() {
            TokenValue::Identifier(identifier) => identifier,
            _ => unreachable!(),
        };

        parser
            .tokens
            .expect(TokenKind::Tilde, "expected a struct member type")?;

        let member_type = parser.parse_typing(BindingPower::None)?;

        let member = StructMember::at_multiple(
            vec![identifier.span, member_type.span],
            (identifier_name, member_type),
        );
        members.push(member);
    }

    parser
        .tokens
        .expect(TokenKind::CurlyClose, "expected the end of a struct")?;

    Ok(Statement::at_multiple(
        vec![identifier.span],
        StatementValue::Struct(StructDeclaration {
            name: identifier_name,
            span: identifier.span,
            members,
        }),
    ))
}

fn parse_intrinsic_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
) -> ParserResult<Statement<'ast>> {
    let intrinsic = module::parse_intrinsic_function(parser, identifier.clone())?;

    Ok(Statement::at_multiple(
        vec![identifier.span, intrinsic.span],
        StatementValue::Intrinsic(intrinsic),
    ))
}

fn parse_variable_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
    identifier_name: Cow<'ast, str>,
) -> ParserResult<Statement<'ast>> {
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

pub fn parse_while_loop<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Statement<'ast>> {
    parser
        .tokens
        .expect(TokenKind::While, "expected a while statement")?;

    let condition = parser.parse_expression(BindingPower::None)?;
    let body = parser.parse_statement(false)?;

    Ok(Statement::at_multiple(
        vec![condition.span, body.span],
        StatementValue::WhileLoop(condition, Box::new(body)),
    ))
}
