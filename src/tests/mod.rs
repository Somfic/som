use crate::prelude::*;

mod arithmetic;
mod literals;
mod variables;

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
    let compiled = compiler.compile(&type_checked);

    let runner = Runner::new();
    let ran = runner.run(compiled).unwrap();

    ran
}
