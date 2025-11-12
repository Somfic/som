#![allow(dead_code)]
#![allow(unused_variables)]

use std::{path::PathBuf, sync::Arc};

pub mod ast;
mod lexer;
mod parser;
pub use parser::Parser;

pub type Result<T> = std::result::Result<T, Error>;

pub enum Source<'input> {
    Raw(&'input str),
    File(PathBuf, &'input str),
}

impl<'input> Source<'input> {
    pub fn get(&self) -> &'input str {
        match self {
            Source::Raw(source) => source,
            Source::File(_, source) => source,
        }
    }

    /// Get a source identifier for error messages
    pub fn identifier(&self) -> Arc<str> {
        match self {
            Source::Raw(_) => "<input>".into(),
            Source::File(path, _) => path.to_str().unwrap_or("<unknown>").into(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    LexicalError(String),
    ParserError(String),
}
