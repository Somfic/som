mod ast;
pub use ast::*;
use target_lexicon::Triple;

mod borrow_check;
mod code_gen;
mod diagnostics;
mod lexer;
mod linker;
mod parser;
mod runner;
mod span;
mod type_check;

use crate::{
    borrow_check::BorrowChecker, code_gen::Codegen, linker::Linker, runner::Runner,
    type_check::TypeInferencer,
};
pub use diagnostics::{Diagnostic, Label, Severity};
pub use span::{Position, Source, Span};

use std::sync::Arc;

fn main() {
    let source_text = r#"

    fn main() -> i32 {
       1
    }

    "#;

    let mut diagnostics: Vec<Diagnostic> = vec![];

    let source = Arc::new(Source::from_raw(source_text));
    let (ast, parse_errors) = parser::parse(source.clone());

    for error in &parse_errors {
        diagnostics.push(error.to_diagnostic());
    }

    let inferencer = TypeInferencer::new();
    let typed_ast = inferencer.check_program(ast);
    for error in &typed_ast.errors {
        diagnostics.push(error.to_diagnostic(&typed_ast.ast));
    }

    let mut borrow_checker = BorrowChecker::new(&typed_ast);
    for error in &borrow_checker.check_program() {
        diagnostics.push(error.to_diagnostic(&typed_ast));
    }

    if !diagnostics.is_empty() {
        for diagnostic in &diagnostics {
            println!("{}\n", diagnostic);
        }
        std::process::exit(1);
    }

    let codegen = match Codegen::new(&typed_ast, Triple::host()) {
        Ok(cg) => cg,
        Err(diagnostic) => {
            println!("{}\n", diagnostic);
            std::process::exit(1);
        }
    };

    let product = match codegen.compile() {
        Ok(p) => p,
        Err(diagnostic) => {
            println!("{}\n", diagnostic);
            std::process::exit(1);
        }
    };

    let linker = Linker::new("som");
    let executable = match linker.link_object(product) {
        Ok(exe) => exe,
        Err(diagnostic) => {
            println!("{}\n", diagnostic);
            std::process::exit(1);
        }
    };

    let runner = Runner::new(executable);
    match runner.run() {
        Ok(status) => {
            println!("exited with {}", status.code().unwrap_or(0));
            std::process::exit(status.code().unwrap_or(0));
        }
        Err(diagnostic) => {
            println!("{}\n", diagnostic);
            std::process::exit(1);
        }
    }
}
