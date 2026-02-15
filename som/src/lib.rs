mod ast;
pub use ast::*;

pub mod arena;
pub mod borrow_check;
mod code_gen;
pub mod diagnostics;
pub mod lexer;
mod linker;
pub mod parser;
mod program;
pub mod resolve;
mod runner;
pub mod scope;
mod span;
mod std;
pub mod type_check;

pub use borrow_check::BorrowChecker;
pub use code_gen::Codegen;
pub use diagnostics::{Diagnostic, Highlight, Label, Related, Severity};
pub use linker::Linker;
pub use program::{ProgramError, ProgramLoader};
pub use resolve::{DefId, DefKind, Definition, NameResolver, ResolvedAst};
pub use runner::Runner;
pub use span::{Position, Source, Span};
pub use std::{BundledFile, BundledModule, get_bundled_module};
pub use type_check::TypeInferencer;
