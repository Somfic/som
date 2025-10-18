use crate::{lowering::Lowering, prelude::*};

/// Test helper for error conditions
/// Returns true if compilation fails at any stage (lexer, parser, type checker, compiler, or runner)
pub fn expect_error(source: &str) -> bool {
    let source = miette::NamedSource::new("test", source.to_string());

    let lexer = Lexer::new(source.inner().as_str());

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(_) => return true, // Parser error occurred
    };

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(_) => return true, // Type checker error occurred
    };

    let mut lowering = Lowering::new();
    let lowered = lowering.lower(type_checked);
    let metadata = lowering.metadata;

    let mut compiler = Compiler::new(metadata);
    let (compiled, return_type) = match compiler.compile(&lowered) {
        Ok(result) => result,
        Err(_) => return true, // Compiler error occurred
    };

    let runner = Runner::new();
    match runner.run(compiled, &return_type) {
        Ok(_) => false, // No error occurred
        Err(_) => true, // Runner error occurred
    }
}

/// Test helper for specific error types
/// Returns the error type name if an error occurs, None if successful
pub fn get_error_type(source: &str) -> Option<String> {
    let source = miette::NamedSource::new("test", source.to_string());

    let lexer = Lexer::new(source.inner().as_str());

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(_) => return Some("ParserError".to_string()),
    };

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(_) => return Some("TypeCheckerError".to_string()),
    };

    let mut lowering = Lowering::new();
    let lowered = lowering.lower(type_checked);
    let metadata = lowering.metadata;

    let mut compiler = Compiler::new(metadata);
    let (compiled, return_type) = match compiler.compile(&lowered) {
        Ok(result) => result,
        Err(_) => return Some("CompilerError".to_string()),
    };

    let runner = Runner::new();
    match runner.run(compiled, &return_type) {
        Ok(_) => None,
        Err(_) => Some("RunnerError".to_string()),
    }
}

/// Test helper that returns the result if successful, or panics with error details
pub fn interpret_strict(source: &str) -> i64 {
    let source = miette::NamedSource::new("test", source.to_string());

    let lexer = Lexer::new(source.inner().as_str());

    let mut parser = Parser::new(lexer);
    let parsed = parser.parse().expect("Parser should succeed");

    let mut type_checker = TypeChecker::new();
    let type_checked = type_checker
        .check(&parsed)
        .expect("Type checker should succeed");

    let mut lowering = Lowering::new();
    let lowered = lowering.lower(type_checked);
    let metadata = lowering.metadata;

    let mut compiler = Compiler::new(metadata);
    let (compiled, return_type) = compiler
        .compile(&lowered)
        .expect("Compiler should succeed");

    let runner = Runner::new();
    runner
        .run(compiled, &return_type)
        .expect("Runner should succeed")
}
