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

    let next_token = parser.tokens.peek();

    if next_token.is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::Type)
    }) {
        return parse_type_declaration(parser, identifier);
    }

    if next_token.is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::Function)
    }) {
        return parse_function_declaration(parser, identifier);
    }

    let declaration = parser.parse_expression(BindingPower::None)?;

    let declaration = match declaration.value {
        ExpressionValue::StructConstructor {
            identifier,
            arguments,
        } => {
            let struct_type = TypingValue::Symbol(identifier.clone()).with_span(identifier.span);

            StatementValue::StructDeclaration {
                identifier: identifier.clone(),
                struct_type,
                explicit_type,
                parameters: arguments,
            }
        }
        _ => StatementValue::VariableDeclaration(identifier.clone(), explicit_type, declaration),
    };

    Ok(declaration.with_span(identifier.span))
}

fn parse_function_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Identifier<'ast>,
) -> ParserResult<Statement<'ast>> {
    let function = module::parse_function(parser, identifier.clone())?;

    let span = identifier.span.combine(function.span);

    Ok(StatementValue::FunctionDeclaration(function).with_span(span))
}

fn parse_type_declaration<'ast>(
    parser: &mut Parser<'ast>,
    identifier: Identifier<'ast>,
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

    Ok(StatementValue::TypeDeclaration(identifier, ty).with_span(span))
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
