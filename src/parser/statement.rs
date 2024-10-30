use crate::lexer::{TokenKind, TokenValue};

use super::{ast::Statement, expression, lookup::BindingPower, Parser};
use miette::{Context, Result};

pub fn parse<'de>(parser: &mut Parser<'de>, optional_semicolon: bool) -> Result<Statement<'de>> {
    let token = match parser.lexer.peek().as_ref() {
        Some(Ok(token)) => token,
        Some(Err(err)) => return Err(miette::miette!(err.to_string())), // FIXME: better error handling
        None => {
            return Err(miette::miette! {
                help = "expected a statement",
                "expected a statement"
            })
        }
    };

    let statement_handler = parser.lookup.statement_lookup.get(&token.kind);

    let token_kind = &token.clone();

    let statement = match statement_handler {
        Some(handler) => handler(parser)?,
        None => {
            let expression = expression::parse(parser, BindingPower::None)
                .wrap_err("while parsing a statement")?;

            if !optional_semicolon {
                parser
                    .lexer
                    .expect(
                        TokenKind::Semicolon,
                        "expected a semicolon at the end of an expression",
                    )
                    .wrap_err(format!("while parsing for {}", token_kind))?;
            }

            Statement::Expression(expression)
        }
    };

    Ok(statement)
}

pub fn let_<'de>(parser: &mut Parser<'de>) -> Result<Statement<'de>> {
    parser
        .lexer
        .expect(TokenKind::Let, "expected a let keyword")?;
    let identifier = parser
        .lexer
        .expect(TokenKind::Identifier, "expected an identifier")?;
    let identifier = match identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };
    parser
        .lexer
        .expect(TokenKind::Equal, "expected an equal sign")?;
    let expression = expression::parse(parser, BindingPower::None)?;

    parser
        .lexer
        .expect(TokenKind::Semicolon, "expected a semicolon")?;

    Ok(Statement::Assignment(identifier, expression))
}

pub fn struct_<'de>(parser: &mut Parser<'de>) -> Result<Statement<'de>> {
    parser
        .lexer
        .expect(TokenKind::Struct, "expected a struct keyword")?;

    let identifier = parser
        .lexer
        .expect(TokenKind::Identifier, "expected an identifier")?;

    let identifier = match identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    parser.lexer.expect(TokenKind::Colon, "expected a colon")?;

    let mut fields = vec![];

    while parser.lexer.peek().map_or(false, |token| {
        token
            .as_ref()
            .map_or(false, |token| token.kind != TokenKind::Semicolon)
    }) {
        if !fields.is_empty() {
            parser.lexer.expect(TokenKind::Comma, "expected a comma")?;
        }

        let field = parser
            .lexer
            .expect(TokenKind::Identifier, "expected an identifier")?;

        let field = match field.value {
            TokenValue::Identifier(field) => field,
            _ => unreachable!(),
        };

        fields.push(field);
    }

    parser
        .lexer
        .expect(TokenKind::Semicolon, "expected a semicolon")?;

    Ok(Statement::Struct {
        name: identifier,
        fields,
    })
}

pub fn enum_<'de>(parser: &mut Parser<'de>) -> Result<Statement<'de>> {
    parser
        .lexer
        .expect(TokenKind::Enum, "expected an enum keyword")?;

    let identifier = parser
        .lexer
        .expect(TokenKind::Identifier, "expected an identifier")?;

    let identifier = match identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    parser.lexer.expect(TokenKind::Colon, "expected a colon")?;

    let mut variants = vec![];

    while parser.lexer.peek().map_or(false, |token| {
        token
            .as_ref()
            .map_or(false, |token| token.kind != TokenKind::Semicolon)
    }) {
        if !variants.is_empty() {
            parser.lexer.expect(TokenKind::Comma, "expected a comma")?;
        }

        let variant = parser
            .lexer
            .expect(TokenKind::Identifier, "expected an identifier")?;

        let variant = match variant.value {
            TokenValue::Identifier(variant) => variant,
            _ => unreachable!(),
        };

        variants.push(variant);
    }

    parser
        .lexer
        .expect(TokenKind::Semicolon, "expected a semicolon")?;

    Ok(Statement::Enum {
        name: identifier,
        variants,
    })
}

pub fn function_<'de>(parser: &mut Parser<'de>) -> Result<Statement<'de>> {
    parser
        .lexer
        .expect(TokenKind::Function, "expected a fn keyword")?;

    let identifier = parser
        .lexer
        .expect(TokenKind::Identifier, "expected an identifier")?;

    let identifier = match identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    parser
        .lexer
        .expect(TokenKind::ParenOpen, "expected an open parenthesis")?;

    let mut parameters = vec![];

    while parser.lexer.peek().map_or(false, |token| {
        token
            .as_ref()
            .map_or(false, |token| token.kind != TokenKind::ParenClose)
    }) {
        if !parameters.is_empty() {
            parser.lexer.expect(TokenKind::Comma, "expected a comma")?;
        }

        let parameter = parser
            .lexer
            .expect(TokenKind::Identifier, "expected an identifier")?;

        let parameter = match parameter.value {
            TokenValue::Identifier(parameter) => parameter,
            _ => unreachable!(),
        };

        parameters.push(parameter);
    }

    parser
        .lexer
        .expect(TokenKind::ParenClose, "expected a close parenthesis")?;

    let body = expression::parse(parser, BindingPower::None)?;

    Ok(Statement::Function {
        name: identifier,
        parameters,
        body,
    })
}

pub fn trait_<'de>(parser: &mut Parser<'de>) -> Result<Statement<'de>> {
    parser
        .lexer
        .expect(TokenKind::Trait, "expected a trait keyword")?;

    let identifier = parser
        .lexer
        .expect(TokenKind::Identifier, "expected an identifier")?;

    let identifier = match identifier.value {
        TokenValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    parser.lexer.expect(TokenKind::Colon, "expected a colon")?;

    let mut functions = vec![];

    while parser.lexer.peek().map_or(false, |token| {
        token
            .as_ref()
            .map_or(false, |token| token.kind != TokenKind::Semicolon)
    }) {
        if !functions.is_empty() {
            parser.lexer.expect(TokenKind::Comma, "expected a comma")?;
        }

        parser
            .lexer
            .expect(TokenKind::Function, "expected a fn keyword")?;

        let function = parser
            .lexer
            .expect(TokenKind::Identifier, "expected an identifier")?;

        let function = match function.value {
            TokenValue::Identifier(function) => function,
            _ => unreachable!(),
        };

        parser
            .lexer
            .expect(TokenKind::ParenOpen, "expected an open parenthesis")?;

        let mut parameters = vec![];

        while parser.lexer.peek().map_or(false, |token| {
            token
                .as_ref()
                .map_or(false, |token| token.kind != TokenKind::ParenClose)
        }) {
            if !parameters.is_empty() {
                parser.lexer.expect(TokenKind::Comma, "expected a comma")?;
            }

            let parameter = parser
                .lexer
                .expect(TokenKind::Identifier, "expected an identifier")?;

            let parameter = match parameter.value {
                TokenValue::Identifier(parameter) => parameter,
                _ => unreachable!(),
            };

            parameters.push(parameter);
        }

        functions.push((function, parameters));
    }

    parser
        .lexer
        .expect(TokenKind::Semicolon, "expected a semicolon")?;

    Ok(Statement::Trait {
        name: identifier,
        functions,
    })
}
