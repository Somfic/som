#![allow(dead_code)]
#![allow(unused_variables)]

use std::{path::PathBuf, sync::Arc};

pub mod ast;
mod lexer;
mod parser;
pub use parser::Parser;
mod span;
pub use span::Span;
mod diagnostics;
pub use diagnostics::*;
mod type_check;
pub use type_check::*;

pub type Result<T> = std::result::Result<T, Diagnostic>;

pub trait Phase {
    type TypeInfo: std::fmt::Debug;
}

pub enum Source {
    Raw(Arc<str>),
    File(PathBuf, Arc<str>),
}

impl Source {
    pub fn content(&self) -> Arc<str> {
        match self {
            Source::Raw(source) => source.clone(),
            Source::File(_, source) => source.clone(),
        }
    }

    pub fn identifier(&self) -> Arc<str> {
        match self {
            Source::Raw(_) => "<input>".into(),
            Source::File(path, _) => path.to_str().unwrap_or("<unknown>").into(),
        }
    }

    pub fn from_raw(source: impl Into<Arc<str>>) -> Self {
        Source::Raw(source.into())
    }

    pub fn from_file(file: impl Into<PathBuf>) -> Result<Self> {
        let file = file.into();
        let content = std::fs::read_to_string(&file).map_err(|e| {
            LexicalError::IoError(e)
                .to_diagnostic()
                .with_hint(format!("could not read file '{}'", file.display()))
        })?;
        Ok(Source::File(file, Arc::from(content)))
    }
}
