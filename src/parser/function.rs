use crate::ast::{Expression, ExpressionValue, LambdaSignature};
use crate::prelude::*;
use crate::{ast::Parameter, tokenizer::TokenKind};

use super::{BindingPower, Parser};

pub fn parse_lambda_signature(parser: &mut Parser) -> Result<LambdaSignature> {
    parser
        .tokens
        .expect(TokenKind::Function, "expected a function declaration")?;

    let parameters = parse_optional_lambda_parameters(parser)?;

    let explicit_return_type = if parser.tokens.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::Arrow)
    }) {
        parser
            .tokens
            .expect(TokenKind::Arrow, "expected a return type")?;

        Some(Box::new(parser.parse_typing(BindingPower::None)?))
    } else {
        None
    };

    let span = combine_spans(parameters.iter().map(|p| p.span).collect::<Vec<_>>());

    Ok(LambdaSignature {
        parameters,
        explicit_return_type,
        span,
    })
}

pub fn parse_lambda(parser: &mut Parser) -> Result<Expression> {
    let signature = parse_lambda_signature(parser)?;

    let body = parser.parse_expression(BindingPower::None)?;

    Ok(ExpressionValue::Lambda {
        parameters: signature.parameters,
        explicit_return_type: signature.explicit_return_type,
        body: Box::new(body),
    }
    .with_span(signature.span))
}

fn parse_optional_lambda_parameters(parser: &mut Parser) -> Result<Vec<Parameter>> {
    let token = match parser.tokens.peek().as_ref() {
        Some(Ok(token)) => token,
        Some(Err(err)) => return Err(err.to_vec()),
        None => {
            return Err(vec![miette::diagnostic! {
                help = "expected a type",
                "expected a type"
            }]);
        }
    };

    match token.kind {
        TokenKind::ParenOpen => {
            parser.tokens.next();
            parse_lambda_parameters(parser)
        }
        _ => Ok(Vec::new()),
    }
}

fn parse_lambda_parameters(parser: &mut Parser) -> Result<Vec<Parameter>> {
    let mut parameters = Vec::new();

    loop {
        if parser.tokens.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::ParenClose)
        }) {
            break;
        }

        if !parameters.is_empty() {
            parser
                .tokens
                .expect(TokenKind::Comma, "expected a comma between parameters")?;
        }

        let parameter = parser
            .tokens
            .expect(TokenKind::Identifier, "expected a parameter name")?;

        let parameter_name = match parameter.value {
            crate::tokenizer::TokenValue::Identifier(name) => name,
            _ => unreachable!(),
        };

        parser
            .tokens
            .expect(TokenKind::Tilde, "expected a parameter type")?;

        let parameter_type = parser.parse_typing(BindingPower::None)?;

        parameters.push(Parameter {
            identifier: parameter_name,
            span: parameter.span.combine(parameter_type.span),
            ty: parameter_type.clone(),
        });
    }

    parser.tokens.expect(
        TokenKind::ParenClose,
        "expected the end of a parameter list",
    )?;

    Ok(parameters)
}
