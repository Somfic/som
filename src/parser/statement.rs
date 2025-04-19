use std::collections::HashMap;

use super::{
    module,
    typing::{self, parse_struct},
    BindingPower, Parser,
};
use crate::{
    ast::{
        combine_spans, CombineSpan, ExpressionValue, Identifier, Statement, StatementValue,
        StructMember, Typing, TypingValue,
    },
    tokenizer::{Token, TokenKind, TokenValue},
    ParserResult,
};

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

    let span = combine_spans(statements.iter().map(|s| s.span).collect());

    Ok(StatementValue::Block(statements).with_span(span))
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

    let next_token = parser.tokens.peek();

    if next_token.is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::Type)
    }) {
        return parse_type_declaration(parser, identifier, identifier_name);
    }

    if next_token.is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::Function)
    }) {
        return parse_function_declaration(parser, identifier);
    }

    let declaration = parser.parse_expression(BindingPower::Assignment)?;

    let declaration = match declaration.value {
        ExpressionValue::StructConstructor {
            identifier,
            arguments,
        } => {
            let struct_type = TypingValue::Symbol(identifier.clone()).with_span(declaration.span);

            StatementValue::StructDeclaration(
                identifier_name,
                struct_type,
                explicit_type,
                arguments,
            )
        }
        _ => StatementValue::VariableDeclaration(identifier_name, explicit_type, declaration),
    };

    Ok(declaration.with_span(identifier.span))
}

fn parse_function_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
) -> ParserResult<Statement<'ast>> {
    let function = module::parse_function(parser, identifier.clone())?;

    let span = identifier.span.combine(function.span);

    Ok(StatementValue::FunctionDeclaration(function).with_span(span))
}

fn parse_type_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
    identifier_name: Identifier<'ast>,
) -> ParserResult<Statement<'ast>> {
    parser
        .tokens
        .expect(TokenKind::Type, "expected a type declaration")?;

    // let ty = match parser.tokens.peek().unwrap().map(|t| t.kind) {
    //     TokenKind::CurlyOpen => typing::parse_struct(parser, identifier)?,
    //     TokenKind::Identifier => typing::parse_symbol(parser)?,
    //     _ => todo!("parse type declaration"),
    // };

    let ty = parser.parse_typing(BindingPower::None)?;
    let span = ty.span;

    Ok(StatementValue::TypeDeclaration(identifier_name, ty).with_span(span))
}

fn parse_struct_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
    explicit_type: Option<Typing<'ast>>,
) -> ParserResult<Statement<'ast>> {
    let struct_identifier = parser
        .tokens
        .expect(TokenKind::Identifier, "expected a struct name")?;

    let open = parser
        .tokens
        .expect(TokenKind::CurlyOpen, "expected a struct")?;

    let struct_identifier_name = match struct_identifier.value.clone() {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    let identifier_name = match identifier.value.clone() {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    let struct_type = TypingValue::Symbol(struct_identifier_name).with_span(struct_identifier.span);

    let mut members = HashMap::new();

    while parser.tokens.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind != TokenKind::CurlyClose)
    }) {
        if !members.is_empty() {
            parser.tokens.expect(TokenKind::Comma, "expected a comma")?;
        }

        let identifier = parser
            .tokens
            .expect(TokenKind::Identifier, "expected a member name")?;

        let identifier_name = match identifier.value.clone() {
            TokenValue::Identifier(identifier) => identifier,
            _ => unreachable!(),
        };

        parser
            .tokens
            .expect(TokenKind::Equal, "expected a member value")?;

        let member_value = parser.parse_expression(BindingPower::None)?;

        // TODO: error if the member is already defined
        members.insert(identifier_name, member_value);
    }

    let close = parser
        .tokens
        .expect(TokenKind::CurlyClose, "expected the end of a struct")?;

    Ok(
        StatementValue::StructDeclaration(identifier_name, struct_type, explicit_type, members)
            .with_span(open.span.combine(close.span)),
    )
}

fn parse_intrinsic_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
) -> ParserResult<Statement<'ast>> {
    let intrinsic = module::parse_intrinsic_function(parser, identifier.clone())?;

    let span = identifier.span.combine(intrinsic.span);

    Ok(StatementValue::IntrinsicDeclaration(intrinsic).with_span(span))
}

fn parse_variable_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Token<'ast>,
    identifier_name: Identifier<'ast>,
    explicit_type: Option<Typing<'ast>>,
) -> ParserResult<Statement<'ast>> {
    let expression = parser.parse_expression(BindingPower::Assignment)?;
    let span = identifier.span.combine(expression.span);

    Ok(
        StatementValue::VariableDeclaration(identifier_name, explicit_type, expression)
            .with_span(span),
    )
}

pub fn parse_condition<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Statement<'ast>> {
    parser
        .tokens
        .expect(TokenKind::If, "expected an if statement")?;

    let condition = parser.parse_expression(BindingPower::None)?;
    let body = parser.parse_statement(false)?;

    let span = condition.span.combine(body.span);

    Ok(StatementValue::Condition(condition, Box::new(body)).with_span(span))
}

pub fn parse_while_loop<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Statement<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::While, "expected a while statement")?;

    let condition = parser.parse_expression(BindingPower::None)?;
    let body = parser.parse_statement(false)?;

    let span = token.span.combine(condition.span);

    Ok(StatementValue::WhileLoop(condition, Box::new(body)).with_span(span))
}
