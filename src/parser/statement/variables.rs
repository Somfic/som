use crate::{
    diagnostic::{Diagnostic, Snippet},
    parser::{
        ast::{Statement, Type},
        expression,
        lookup::{BindingPower, Lookup},
        macros::{expect_token, expect_value, optional_token},
        typest, ParseResult,
    },
    scanner::lexeme::TokenType,
};

pub fn register(lookup: &mut Lookup) {
    lookup.add_statement_handler(TokenType::Let, parse_variable);
}

/// Parses a variable signature that might have an explicit type.
/// color or color ~ Color
pub fn parse_variable_signature<'a>(
    parser: &mut super::Parser<'a>,
) -> ParseResult<'a, (String, Option<Type>)> {
    let identifier = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier, Identifier);

    let typest = optional_token!(parser, Tilde)
        .map(|_| typest::parse(parser, BindingPower::None))
        .transpose()?;

    Ok((identifier, typest))
}

/// Parses a variable signature with an explicit type.
/// color ~ Color
pub fn parse_explicit_variable_signature<'a>(
    parser: &mut super::Parser<'a>,
    type_of_variable: impl Into<String>,
) -> ParseResult<'a, (String, Type)> {
    let identifier_token = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier_token, Identifier);

    let typest = optional_token!(parser, Tilde)
        .map(|_| typest::parse(parser, BindingPower::None))
        .transpose()?;

    if typest.is_none() {
        let type_of_variable = type_of_variable.into();

        return Err(Diagnostic::error(identifier, "Expected an explicit type")
            .with_snippet(Snippet::primary_from_token(
                &identifier_token,
                format!("This {} is missing an explicit type", type_of_variable),
            ))
            .with_note(format!(
                "The {} was expected to have an explicit type, but none was declared",
                type_of_variable
            ))
            .with_note(format!(
                "Use {} to specify an explicit type",
                TokenType::Tilde
            )));
    }

    let typest = typest.unwrap();

    Ok((identifier, typest))
}

fn parse_variable<'a>(parser: &mut super::Parser<'a>) -> ParseResult<'a, Statement> {
    expect_token!(parser, Let)?;

    let variable = parse_variable_signature(parser)?;

    expect_token!(parser, Equal)?;

    let expression = expression::parse(parser, BindingPower::None)?;

    expect_token!(parser, Semicolon)?;

    Ok(Statement::Declaration(variable.0, variable.1, expression))
}
