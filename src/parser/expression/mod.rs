use miette::{Result, SourceSpan};

use super::{
    ast::{
        untyped::{Expression, ExpressionValue, Lambda, ParameterDeclaration},
        CombineSpan, Spannable,
    },
    Parser,
};
use crate::{
    lexer::{TokenKind, TokenValue},
    parser::lookup::BindingPower,
};

pub mod binary;
pub mod primitive;
pub mod unary;

pub fn parse<'de>(
    parser: &mut Parser<'de>,
    binding_power: BindingPower,
) -> Result<Expression<'de>> {
    let token = match parser.lexer.peek().as_ref() {
        Some(Ok(token)) => token,
        Some(Err(err)) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
        None => {
            return Err(miette::miette! {
                help = "expected an expression",
                "expected an expression"
            })
        }
    };

    let handler = parser
        .lookup
        .expression_lookup
        .get(&token.kind)
        .ok_or(miette::miette! {
            labels = vec![token.label("expected an expression")],
            help = format!("{} is not an expression", token.kind),
            "expected an expression, found {}", token.kind
        })?;
    let mut lhs = handler(parser)?;

    let mut next_token = parser.lexer.peek();

    while let Some(token) = next_token {
        let token = match token {
            Ok(token) => token,
            Err(err) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
        };

        let token_binding_power = {
            let binding_power_lookup = parser.lookup.binding_power_lookup.clone();
            binding_power_lookup
                .get(&token.kind)
                .unwrap_or(&BindingPower::None)
                .clone()
        };

        if binding_power > token_binding_power {
            break;
        }

        let handler = match parser.lookup.left_expression_lookup.get(&token.kind) {
            Some(handler) => handler,
            None => break,
        };

        parser.lexer.next();

        lhs = handler(parser, lhs, token_binding_power)?;

        next_token = parser.lexer.peek();
    }

    Ok(lhs)
}

pub fn call<'de>(
    parser: &mut Parser<'de>,
    lhs: Expression<'de>,
    _binding_power: BindingPower,
) -> Result<Expression<'de>> {
    let mut arguments = Vec::new();

    while parser.lexer.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind != TokenKind::ParenClose)
    }) {
        if !arguments.is_empty() {
            parser
                .lexer
                .expect(TokenKind::Comma, "expected a comma between arguments")?;
        }

        let argument = parse(parser, BindingPower::None)?;
        arguments.push(argument);
    }

    let close = parser
        .lexer
        .expect(TokenKind::ParenClose, "expected a closing parenthesis")?;

    Ok(Expression::at_multiple(
        vec![lhs.span, close.span],
        ExpressionValue::Call {
            callee: Box::new(lhs.clone()),
            arguments,
        },
    ))
}

pub fn lambda<'de>(parser: &mut Parser<'de>) -> Result<Expression<'de>> {
    parser
        .lexer
        .expect(TokenKind::Pipe, "expected a pipe before lambda arguments")?;

    let mut parameters = Vec::new();

    while parser.lexer.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind != TokenKind::Pipe)
    }) {
        if !parameters.is_empty() {
            parser
                .lexer
                .expect(TokenKind::Comma, "expected a comma between arguments")?;
        }

        let parameter = parser.lexer.expect(
            TokenKind::Identifier,
            "expected an identifier for a lambda argument",
        )?;

        let name = match parameter.value {
            TokenValue::Identifier(v) => v,
            _ => unreachable!(),
        };

        parser
            .lexer
            .expect(TokenKind::Tilde, "expected a tilde after lambda argument")?;

        let explicit_type = super::typing::parse(parser, BindingPower::None)?;

        parameters.push(ParameterDeclaration {
            span: SourceSpan::combine(vec![parameter.span, explicit_type.span]),
            name,
            explicit_type,
        });
    }

    let pipe = parser
        .lexer
        .expect(TokenKind::Pipe, "expected a pipe after lambda arguments")?;

    let body = parse(parser, BindingPower::None)?;

    Ok(Expression::at_multiple(
        vec![pipe.span, body.span],
        ExpressionValue::Lambda(Lambda {
            parameters,
            body: Box::new(body),
        }),
    ))
}
