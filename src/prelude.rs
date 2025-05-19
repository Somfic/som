pub use crate::expressions::binary::BinaryExpression;
pub use crate::expressions::binary::BinaryOperator;
pub use crate::expressions::primary::PrimaryExpression;
pub use crate::parser::lookup::{BindingPower, Lookup};
pub use crate::statements::{Statement, StatementValue};
pub use crate::type_checker::TypeChecker;
pub use crate::types::{Type, TypeKind};
pub use crate::{
    expressions::{Expression, ExpressionValue, TypedExpression},
    lexer::{Lexer, Token, TokenKind, TokenValue},
};
pub use crate::{parser::Parser, statements::TypedStatement};
use miette::LabeledSpan;
pub use miette::SourceSpan;
pub use miette::{Context, Diagnostic};
use std::fmt::Display;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;
pub type Results<T> = std::result::Result<T, Vec<Error>>;

#[derive(Clone, Error, Debug, Diagnostic)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Lexer(#[from] LexerError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Parser(#[from] ParserError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    TypeChecker(#[from] TypeCheckerError),
}

#[derive(Clone, Error, Debug, Diagnostic)]
pub enum LexerError {
    #[error("unexpected character")]
    #[diagnostic()]
    UnexpectedCharacter {
        #[label("this character was not expected")]
        range: (usize, usize),

        #[help]
        help: String,
    },

    #[error("improper number")]
    #[diagnostic()]
    ImproperNumber {
        #[label("this is not a valid number")]
        range: (usize, usize),

        #[help]
        help: String,
    },

    #[error("improper character")]
    #[diagnostic()]
    ImproperCharacter {
        #[label("this is not a valid character")]
        range: (usize, usize),

        #[help]
        help: String,
    },
}

#[derive(Clone, Error, Debug, Diagnostic)]
pub enum ParserError {
    #[error("unexpected token")]
    #[diagnostic()]
    UnexpectedToken {
        #[label("this token was not expected")]
        token: Token,

        #[help]
        help: String,
    },

    #[error("unexpected end of file")]
    #[diagnostic()]
    UnexpectedEndOfFile {
        #[label(collection)]
        labels: Vec<LabeledSpan>,
        #[help]
        help: String,
    },

    #[error("expected expression")]
    #[diagnostic()]
    ExpectedExpression {
        #[label("expected an expression here")]
        token: Token,
        #[help]
        help: String,
    },
}

#[derive(Clone, Error, Debug, Diagnostic)]
pub enum TypeCheckerError {
    #[error("unexpected end of file")]
    #[diagnostic()]
    TypeMismatch {
        #[label(collection, "")]
        labels: Vec<LabeledSpan>,

        #[help]
        help: String,
    },
}

pub fn lexer_unexpected_character(original: char, range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::UnexpectedCharacter {
        help: format!("'{original}' cannot be parsed"),
        range,
    })
}

pub fn lexer_improper_number(original: &str, range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::ImproperNumber {
        help: format!("'{original}' cannot be parsed as a number"),
        range,
    })
}

pub fn lexer_improper_character(original: &str, range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::ImproperCharacter {
        help: format!("'{original}' cannot be parsed as a character"),
        range,
    })
}

pub fn parser_unexpected_token(
    help: impl Into<String>,
    token: &Token,
    expected: &TokenKind,
) -> Error {
    let help = help.into();

    Error::Parser(ParserError::UnexpectedToken {
        help: format!("{help}, but found {}", token.kind),
        token: token.clone(),
    })
}

pub fn parser_unexpected_end_of_file(span: (usize, usize), expected: impl Into<String>) -> Error {
    let expected = expected.into();

    Error::Parser(ParserError::UnexpectedEndOfFile {
        help: format!("expected {expected} but no more tokens were found"),
        labels: vec![LabeledSpan::new(
            Some(format!("expected {expected} here")),
            span.0,
            span.1,
        )],
    })
}

pub fn parser_expected_expression(token: &Token) -> Error {
    Error::Parser(ParserError::ExpectedExpression {
        help: format!("{token} cannot be parsed as an expression"),
        token: token.clone(),
    })
}

pub fn type_checker_type_mismatch(types: Vec<&Type>, help: impl Into<String>) -> Error {
    let distinct_types = types.iter().collect::<std::collections::HashSet<_>>();

    let most_occuring_type = if types.len() <= 2 {
        None
    } else {
        types
            .iter()
            .fold(std::collections::HashMap::new(), |mut acc, ty| {
                *acc.entry(ty.kind).or_insert(0) += 1;
                acc
            })
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(kind, _)| kind)
    };

    let invalid_types = match most_occuring_type {
        Some(most_occuring_type) => distinct_types
            .clone()
            .into_iter()
            .filter(|ty| ty.kind != most_occuring_type)
            .collect::<Vec<_>>(),
        None => distinct_types.clone().into_iter().collect::<Vec<_>>(),
    };

    let generated_help = match most_occuring_type {
        Some(most_occuring_type) => format!("this should probably be {most_occuring_type}"),
        None => format!("but found {}", join_with_and(distinct_types)),
    };

    // if we know the most occuring type, label all the invalid types
    // otherwise, label all the types
    let labels: Vec<_> = match most_occuring_type {
        Some(_) => invalid_types
            .into_iter()
            .map(|ty| LabeledSpan::new(Some(format!("{ty}")), ty.span.offset(), ty.span.len()))
            .collect(),
        None => types
            .into_iter()
            .map(|ty| LabeledSpan::new(Some(format!("{ty}")), ty.span.offset(), ty.span.len()))
            .collect(),
    };

    Error::TypeChecker(TypeCheckerError::TypeMismatch {
        help: format!("{}, {generated_help}", help.into(),),
        labels,
    })
}

pub fn join_with_and<T, I>(items: I) -> String
where
    T: Display,
    I: IntoIterator<Item = T>,
{
    let items: Vec<_> = items.into_iter().collect();
    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            if i == items.len() - 2 {
                format!("{item} and")
            } else if i == items.len() - 1 {
                format!("{item}")
            } else {
                format!("{item},")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
