use crate::lexer::{self, TokenKind, TokenValue};

use super::{
    ast::{
        EnumMemberDeclaration, FunctionHeader, ParameterDeclaration, Statement,
        StructMemberDeclaration,
    },
    expression,
    lookup::BindingPower,
    typing, Parser,
};
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
        .expect(TokenKind::Identifier, "expected a variable name")?;
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

    Ok(Statement::Assignment {
        name: identifier,
        value: expression,
    })
}

pub fn struct_<'de>(parser: &mut Parser<'de>) -> Result<Statement<'de>> {
    parser
        .lexer
        .expect(TokenKind::Struct, "expected a struct keyword")?;

    let identifier = parser
        .lexer
        .expect(TokenKind::Identifier, "expected a struct name")?;

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
            parser
                .lexer
                .expect(TokenKind::Comma, "expected a comma between fields")?;
        }

        let field = parser
            .lexer
            .expect(TokenKind::Identifier, "expected a field name")?;

        let field = match field.value {
            TokenValue::Identifier(field) => field,
            _ => unreachable!(),
        };

        parser.lexer.expect(TokenKind::Tilde, "expected a tilde")?;

        let explicit_type = typing::parse(parser, BindingPower::None)?;

        fields.push(StructMemberDeclaration {
            name: field,
            explicit_type,
        });
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
        .expect(TokenKind::Identifier, "expected an enum name")?;

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
            parser
                .lexer
                .expect(TokenKind::Comma, "expected a comma between enum members")?;
        }

        let variant = parser
            .lexer
            .expect(TokenKind::Identifier, "expected an enum member name")?;

        let variant = match variant.value {
            TokenValue::Identifier(variant) => variant,
            _ => unreachable!(),
        };

        variants.push(EnumMemberDeclaration {
            name: variant,
            value_type: None,
        });
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
        .expect(TokenKind::Function, "expected a function keyword")?;

    let identifier = parser
        .lexer
        .expect(TokenKind::Identifier, "expected function name")?;

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
            parser
                .lexer
                .expect(TokenKind::Comma, "expected a comma between parameters")?;
        }

        let parameter = parser
            .lexer
            .expect(TokenKind::Identifier, "expected a parameter name")?;

        let parameter = match parameter.value {
            TokenValue::Identifier(parameter) => parameter,
            _ => unreachable!(),
        };

        parser.lexer.expect(TokenKind::Tilde, "expected a tilde")?;

        let explicit_type = typing::parse(parser, BindingPower::None)?;

        parameters.push(ParameterDeclaration {
            name: parameter,
            explicit_type,
        });
    }

    parser
        .lexer
        .expect(TokenKind::ParenClose, "expected a close parenthesis")?;

    let explicit_return_type = match parser.lexer.peek_expect(TokenKind::Tilde) {
        None => None,
        Some(_) => {
            parser.lexer.next();
            Some(typing::parse(parser, BindingPower::None)?)
        }
    };

    let body = expression::parse(parser, BindingPower::None)?;

    Ok(Statement::Function {
        header: FunctionHeader {
            name: identifier,
            parameters,
        },
        body,
        explicit_return_type,
    })
}

pub fn trait_<'de>(parser: &mut Parser<'de>) -> Result<Statement<'de>> {
    parser
        .lexer
        .expect(TokenKind::Trait, "expected a trait keyword")?;

    let identifier = parser
        .lexer
        .expect(TokenKind::Identifier, "expected a trait name")?;

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
            parser
                .lexer
                .expect(TokenKind::Comma, "expected a comma between functions")?;
        }

        parser
            .lexer
            .expect(TokenKind::Function, "expected a function keyword")?;

        let function = parser
            .lexer
            .expect(TokenKind::Identifier, "expected a function name")?;

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
                parser
                    .lexer
                    .expect(TokenKind::Comma, "expected a comma in between arguments")?;
            }

            let parameter = parser
                .lexer
                .expect(TokenKind::Identifier, "expected an argument name")?;

            let parameter = match parameter.value {
                TokenValue::Identifier(parameter) => parameter,
                _ => unreachable!(),
            };

            parser.lexer.expect(TokenKind::Tilde, "expected a tilde")?;

            let explicit_type = typing::parse(parser, BindingPower::None)?;

            parameters.push(ParameterDeclaration {
                name: parameter,
                explicit_type,
            });
        }

        parser
            .lexer
            .expect(TokenKind::ParenClose, "expected a close parenthesis")?;

        functions.push(FunctionHeader {
            name: function,
            parameters,
        })
    }

    parser
        .lexer
        .expect(TokenKind::Semicolon, "expected a semicolon")?;

    Ok(Statement::Trait {
        name: identifier,
        functions,
    })
}
