mod ast;
pub use ast::*;

mod arena;
pub mod borrow_check;
mod code_gen;
pub mod diagnostics;
pub mod lexer;
mod linker;
pub mod parser;
mod runner;
mod scope;
mod span;
pub mod type_check;

pub use borrow_check::BorrowChecker;
pub use code_gen::Codegen;
pub use diagnostics::{Diagnostic, Label, Severity};
pub use linker::Linker;
pub use runner::Runner;
pub use span::{Position, Source, Span};
pub use type_check::TypeInferencer;
