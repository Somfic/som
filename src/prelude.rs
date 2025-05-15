pub use crate::lexer::Token;
use miette::Diagnostic;
pub use miette::SourceSpan;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Vec<Error>>;

#[derive(Error, Debug, Diagnostic)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Lexer(LexerError),
}

#[derive(Error, Debug, Diagnostic)]
pub enum LexerError {
    #[error("unexpected character")]
    #[diagnostic()]
    UnexpectedCharacter {
        #[label("primary")]
        token: Token,

        #[help]
        help: Option<String>,
    },
}
