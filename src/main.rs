mod ast;
pub use ast::*;

mod borrow_check;
mod diagnostics;
mod lexer;
mod parser;
mod span;
mod type_check;

use crate::{borrow_check::BorrowChecker, type_check::TypeInferencer};
pub use diagnostics::{Diagnostic, Label, Severity};
pub use span::{Position, Source, Span};

use std::sync::Arc;

fn main() {
    let source_text = r#"
    

    fn identity<T>(x: T) -> T { x }

    fn main() {
       identity(1) + false;
    }

    "#;

    let source = Arc::new(Source::from_raw(source_text));
    let (ast, parse_errors) = parser::parse(source.clone());

    for error in &parse_errors {
        let diagnostic = error.to_diagnostic();
        println!("{}\n", diagnostic);
    }

    let inferencer = TypeInferencer::new();
    let typed_ast = inferencer.check_program(ast);
    for error in &typed_ast.errors {
        let diagnostic = error.to_diagnostic(&typed_ast.ast);
        println!("{}\n", diagnostic);
    }

    let mut borrow_checker = BorrowChecker::new(&typed_ast);

    for error in &borrow_checker.check_program() {
        let diagnostic = error.to_diagnostic(&typed_ast);
        println!("{}\n", diagnostic);
    }
}
