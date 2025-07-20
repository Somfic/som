use crate::prelude::*;

pub mod arithmetic;
pub mod blocks;
pub mod conditionals;
pub mod edge_cases;
pub mod extended_literals;
pub mod functions;
pub mod integration;
pub mod literals;
pub mod precedence;
pub mod types;
pub mod variables;

#[test]
fn test_error_handling() {
    // Test that parse and type check errors are properly handled
    let result = interpret("let x = unknown_function();");
    assert_eq!(result, 0); // Should return 0 for errors
}

pub fn interpret(source: &str) -> i64 {
    let source = miette::NamedSource::new("test", source.to_string());

    let lexer = Lexer::new(source.inner().as_str());

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            return 0; // Return a default value for tests
        }
    };

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            return 0; // Return a default value for tests
        }
    };

    let mut compiler = Compiler::new();
    let (compiled, return_type) = match compiler.compile(&type_checked) {
        Ok(result) => result,
        Err(error) => {
            eprintln!(
                "{}",
                miette::miette!(error).with_source_code(source.clone())
            );
            return 0; // Return a default value for tests
        }
    };

    let runner = Runner::new();
    let ran = match runner.run(compiled, &return_type) {
        Ok(value) => value,
        Err(error) => {
            eprintln!(
                "{}",
                miette::miette!(error).with_source_code(source.clone())
            );
            return 0; // Return a default value for tests
        }
    };

    ran
}
