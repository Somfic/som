pub use crate::lexer::*;
use miette::Diagnostic;
pub use miette::SourceSpan;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;
pub type Results<T> = std::result::Result<T, Vec<Error>>;

#[derive(Error, Debug, Diagnostic)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Lexer(LexerError),
}

#[derive(Error, Debug, Diagnostic)]
pub enum LexerError {
    #[error("unexpected token")]
    #[diagnostic()]
    UnexpectedToken {
        #[label("this token was not expected")]
        token: Token,

        #[help]
        help: String,
    },

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

pub fn unexpected_token(token: &Token, expected: &TokenKind) -> Error {
    Error::Lexer(LexerError::UnexpectedToken {
        help: format!("expected token of kind {expected:?}, found {}", token.kind),
        token: token.clone(),
    })
}

pub fn unexpected_character(original: char, range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::UnexpectedCharacter {
        help: format!("'{original}' cannot be parsed"),
        range,
    })
}

pub fn improper_number(original: &str, range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::ImproperNumber {
        help: format!("'{original}' cannot be parsed as a number"),
        range,
    })
}

pub fn improper_character(original: &str, range: (usize, usize)) -> Error {
    Error::Lexer(LexerError::ImproperCharacter {
        help: format!("'{original}' cannot be parsed as a character"),
        range,
    })
}
