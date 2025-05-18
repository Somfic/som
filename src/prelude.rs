pub use crate::expressions::binary::BinaryExpression;
pub use crate::expressions::binary::BinaryOperator;
pub use crate::expressions::primary::PrimaryExpression;
pub use crate::parser::lookup::{BindingPower, Lookup};
pub use crate::statements::{Statement, StatementValue};
pub use crate::types::{Type, TypeKind};
pub use crate::{
    expressions::{Expression, ExpressionValue, TypedExpression},
    lexer::{Lexer, Token, TokenKind, TokenValue},
};
pub use crate::{parser::Parser, statements::TypedStatement};
pub use miette::SourceSpan;
pub use miette::{Context, Diagnostic};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, miette::Report>;
pub type Results<T> = std::result::Result<T, Vec<miette::Report>>;

#[derive(Error, Debug, Diagnostic)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Lexer(LexerError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Parser(ParserError),
}

#[derive(Error, Debug, Diagnostic)]
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

#[derive(Error, Debug, Diagnostic)]
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
        #[label("expected more tokens here")]
        span: (usize, usize),
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

pub fn parser_unexpected_token(token: &Token, expected: &TokenKind) -> Error {
    Error::Parser(ParserError::UnexpectedToken {
        help: format!("expected {expected}, found {}", token.kind),
        token: token.clone(),
    })
}

pub fn parser_unexpected_end_of_file(span: (usize, usize), expected: impl Into<String>) -> Error {
    Error::Parser(ParserError::UnexpectedEndOfFile {
        help: format!("expected {} but no more tokens were found", expected.into()),
        span,
    })
}

pub fn parser_expected_expression(token: &Token) -> Error {
    Error::Parser(ParserError::ExpectedExpression {
        help: format!("{token} cannot be parsed as an expression"),
        token: token.clone(),
    })
}
