mod ast;
pub use ast::*;

mod arena;
pub mod borrow_check;
mod code_gen;
pub mod diagnostics;
pub mod lexer;
mod linker;
mod program;
pub mod parser;
mod runner;
mod scope;
mod span;
mod std;
pub mod type_check;

pub use borrow_check::BorrowChecker;
pub use code_gen::Codegen;
pub use diagnostics::{Diagnostic, Label, Severity};
pub use linker::Linker;
pub use program::{ProgramError, ProgramLoader};
pub use runner::Runner;
pub use span::{Position, Source, Span};
pub use std::{get_bundled_module, BundledFile, BundledModule};
pub use type_check::TypeInferencer;
