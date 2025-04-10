use std::collections::HashMap;

use crate::{
    ast::{Expression, FunctionDeclaration},
    tokenizer::TokenKind,
    ParserResult,
};

use super::{BindingPower, Parser};

pub fn parse_function<'ast>(
    parser: &mut Parser<'ast>,
) -> ParserResult<FunctionDeclaration<'ast, Expression<'ast>>> {
    parser
        .tokens
        .expect(TokenKind::Function, "expected a function declaration")?;

    let name_token = parser
        .tokens
        .expect(TokenKind::Identifier, "expected a function name")?;

    let name = match name_token.value {
        crate::tokenizer::TokenValue::Identifier(name) => name,
        _ => unreachable!(),
    };

    parser.tokens.expect(
        TokenKind::ParenOpen,
        "expected the start of a parameter list",
    )?;

    let mut parameters = HashMap::new();

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

        let parameter = match parameter.value {
            crate::tokenizer::TokenValue::Identifier(name) => name,
            _ => unreachable!(),
        };

        parser
            .tokens
            .expect(TokenKind::Tilde, "expected a parameter type")?;

        let parameter_type = parser.parse_typing(BindingPower::None)?;

        parameters.insert(parameter, parameter_type);
    }

    parser.tokens.expect(
        TokenKind::ParenClose,
        "expected the end of a parameter list",
    )?;

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
        name,
        span: name_token.span,
        parameters,
        body: expression,
        return_type,
    })
}
