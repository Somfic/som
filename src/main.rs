mod ast;
pub use ast::*;

mod diagnostics;
mod lexer;
mod parser;
mod span;

use crate::type_check::TypeInferencer;
pub use diagnostics::{Diagnostic, Label, Severity};
pub use span::{Position, Source, Span};
mod type_check;

use std::sync::Arc;

fn main() {
    // Parse source code
    let source_text = r#"

    fn add(x: i32, y: i32) -> bool {
        x + y + 1
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
                // Create diagnostic with labels
                let mut diagnostic = Diagnostic::error(error.to_diagnostic_message())
                    .with_label(Label::primary(span.clone(), "error occurs here"));

                // Check if there's a secondary span to show
                match error {
                    crate::type_check::TypeError::Mismatch {
                        expected_type_id: Some(type_id),
                        ..
                    } => {
                        if let Some(type_span) = typed_ast.ast.type_spans.get(type_id) {
                            diagnostic = diagnostic.with_label(Label::secondary(
                                type_span.clone(),
                                "expected type defined here",
                            ));
                        }
                    }
                    _ => {}
                };

                println!("{}\n", diagnostic);
            } else {
                println!("  At {:?}: {:?}\n", expr_id, error);
            }
        }
    }
}
