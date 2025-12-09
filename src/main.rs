mod ast;
pub use ast::*;

mod lexer;
mod parser;
mod span;

use crate::type_check::TypeInferencer;
pub use span::Span;
mod type_check;

fn main() {
    // Parse source code
    let source = r#"fn add(x: i32, y: i32) -> bool {
    x > y
}"#;

    let parser = parser::Parser::new(source);
    let (green, parse_errors) = parser.parse();

    if !parse_errors.is_empty() {
        for error in &parse_errors {
            let formatted = span::format_error(source, error.span, &error.message);
            println!("{}\n", formatted);
        }
    }

    let ast = parser::cst::to_ast(green);

    // Type check the entire program
    let inferencer = TypeInferencer::new();
    let typed_ast = inferencer.check_program(ast);

    if !typed_ast.errors.is_empty() {
        for (expr_id, error) in &typed_ast.errors {
            if let Some(span) = typed_ast.ast.expr_spans.get(expr_id) {
                // Check if there's a secondary span to show
                let secondary_spans = match error {
                    crate::type_check::TypeError::Mismatch {
                        expected_type_id: Some(type_id),
                        ..
                    } => {
                        if let Some(type_span) = typed_ast.ast.type_spans.get(type_id) {
                            vec![(*type_span, "expected type defined here")]
                        } else {
                            vec![]
                        }
                    }
                    _ => vec![],
                };

                let formatted = span::format_error_with_secondary(
                    source,
                    *span,
                    &format!("{:?}", error),
                    &secondary_spans,
                );
                println!("{}\n", formatted);
            } else {
                println!("  At {:?}: {:?}\n", expr_id, error);
            }
        }
    }
}
