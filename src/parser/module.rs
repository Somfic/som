use crate::{
    ast::{Expression, FunctionDeclaration, Statement},
    tokenizer::TokenKind,
    ParserResult,
};

use super::Parser;

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

    let mut parameters = vec![];

    loop {
        if parser.tokens.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::ParenClose)
        }) {
            break;
        }

        let parameter = parser
            .tokens
            .expect(TokenKind::Identifier, "expected a parameter name")?;

        let parameter = match parameter.value {
            crate::tokenizer::TokenValue::Identifier(name) => name,
            _ => unreachable!(),
        };

        parameters.push(parameter);
    }

    todo!()
}
