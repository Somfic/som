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
    // Parse source code
    let source_text = r#"

    fn main() {
       let x = 10;
       let r = &x;
       r + 1
    }

    "#;

    let source = Arc::new(Source::from_raw(source_text));
    let (ast, parse_errors) = parser::parse(source.clone());

    if !parse_errors.is_empty() {
        for error in &parse_errors {
            let diagnostic = error.to_diagnostic();
            println!("{}\n", diagnostic);
        }
    }

    // Type check the entire program
    let inferencer = TypeInferencer::new();
    let typed_ast = inferencer.check_program(ast);

    if !typed_ast.errors.is_empty() {
        for (expr_id, error) in &typed_ast.errors {
            if let Some(span) = typed_ast.ast.expr_spans.get(expr_id) {
                let mut diagnostic = Diagnostic::error(error.to_diagnostic_message())
                    .with_label(Label::primary(span.clone(), "error occurs here"));

                if let crate::type_check::TypeError::Mismatch {
                    expected_type_id: Some(type_id),
                    ..
                } = error
                    && let Some(type_span) = typed_ast.ast.type_spans.get(type_id)
                {
                    diagnostic = diagnostic.with_label(Label::secondary(
                        type_span.clone(),
                        "expected type defined here",
                    ));
                };

                println!("{}\n", diagnostic);
            } else {
                println!("  At {:?}: {:?}\n", expr_id, error);
            }
        }
    }

    let mut borrow_checker = BorrowChecker::new(&typed_ast);
    let errors = borrow_checker.check_program();

    for error in &errors {
        let diagnostic = error.to_diagnostic(&typed_ast);
        println!("{}\n", diagnostic);
    }
}
