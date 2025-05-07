
use crate::{
    ast::{
        CombineSpan, FunctionDeclaration, Identifier, IntrinsicFunctionDeclaration,
        Parameter,
    },
    tokenizer::{Token, TokenKind, TokenValue},
    Result,
};

use super::{BindingPower, Parser};

pub fn parse_module_intrinsic_function(
    parser: &mut Parser,
) -> Result<IntrinsicFunctionDeclaration> {
    let identifier = parser
        .tokens
        .expect(TokenKind::Identifier, "expected a function name")?;

    parse_intrinsic_function(parser, identifier)
}

pub fn parse_intrinsic_function(
    parser: &mut Parser,
    identifier: Token,
) -> Result<IntrinsicFunctionDeclaration> {
    parser.tokens.expect(
        TokenKind::Intrinsic,
        "expected an intrinsic function declaration",
    )?;

    parser.tokens.expect(
        TokenKind::Function,
        "expected an intrinsic function declaration",
    )?;

    let identifier_name = match identifier.value.clone() {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    let parameters = parse_optional_function_parameters(parser)?;

    parser
        .tokens
        .expect(TokenKind::Arrow, "expected a return type")?;

    let return_type = parser.parse_typing(BindingPower::None)?;

    Ok(IntrinsicFunctionDeclaration {
        identifier: identifier_name,
        span: identifier.span,
        parameters,
        return_type,
    })
}

pub fn parse_module_function(
    parser: &mut Parser,
) -> Result<FunctionDeclaration> {
    let identifier = parser
        .tokens
        .expect(TokenKind::Identifier, "expected a function name")?;

    let identifier = Identifier::from_token(&identifier)?;

    parse_function(parser, identifier)
}

pub fn parse_function(
    parser: &mut Parser,
    identifier: Identifier,
) -> Result<FunctionDeclaration> {
    parser
        .tokens
        .expect(TokenKind::Function, "expected a function declaration")?;

    let parameters = parse_optional_function_parameters(parser)?;

    let return_type = if parser.tokens.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::Arrow)
    }) {
        parser
            .tokens
            .expect(TokenKind::Arrow, "expected a return type")?;

        Some(parser.parse_typing(BindingPower::None)?)
    } else {
        None
    };

    let expression = parser.parse_expression(BindingPower::None)?;

    Ok(FunctionDeclaration {
        span: identifier.span,
        identifier,
        parameters,
        body: expression,
        explicit_return_type: return_type,
    })
}

fn parse_optional_function_parameters(
    parser: &mut Parser,
) -> Result<Vec<Parameter>> {
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
            parse_function_parameters(parser)
        }
        _ => Ok(Vec::new()),
    }
}

fn parse_function_parameters(
    parser: &mut Parser,
) -> Result<Vec<Parameter>> {
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
