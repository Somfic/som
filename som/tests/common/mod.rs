//! Common test utilities for compiler phase testing.
//!
//! Each phase helper runs all previous phases and panics if they fail,
//! ensuring tests focus on the specific phase being tested.

use som::borrow_check::BorrowError;
use som::lexer::{Token, TokenKind, lex};
use som::parser::{self, ParseError};
use som::type_check::TypeError;
use som::{BorrowChecker, Codegen, Diagnostic, Linker, Runner, Source, TypeInferencer, TypedAst};
use std::path::Path;
use std::sync::Arc;
use target_lexicon::Triple;
use tempfile::NamedTempFile;

/// Result of lexing - just the tokens, no errors possible at this phase
pub fn test_lex(source: &str) -> Vec<Token> {
    let source = Arc::new(Source::from_raw(source));
    lex(source)
}

/// Filters whitespace tokens for easier assertions
pub fn filter_whitespace(tokens: &[Token]) -> Vec<&Token> {
    tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .collect()
}

/// Result of parsing - returns AST and parse errors for assertion
/// Does NOT panic on parse errors since parser tests need to check them
pub fn test_parse(source: &str) -> (som::Ast, Vec<ParseError>) {
    let source = Arc::new(Source::from_raw(source));
    parser::parse(source)
}

/// Parse and panic on errors - for use in later phases
pub fn parse_or_panic(source: &str) -> som::Ast {
    let (ast, errors) = test_parse(source);
    if !errors.is_empty() {
        panic!(
            "Parse errors (test bug - source should be valid):\n{:?}",
            errors
        );
    }
    ast
}

/// Type check phase - parses first and panics on parse errors
/// Returns TypedAst which contains type errors for assertion
pub fn test_type_check(source: &str) -> TypedAst {
    let ast = parse_or_panic(source);
    let inferencer = TypeInferencer::new();
    inferencer.check_program(ast)
}

/// Type check and panic on errors - for use in later phases
pub fn type_check_or_panic(source: &str) -> TypedAst {
    let typed_ast = test_type_check(source);
    if !typed_ast.errors.is_empty() {
        let errors: Vec<String> = typed_ast
            .errors
            .iter()
            .map(|e| format!("{:?}", e))
            .collect();
        panic!(
            "Type errors (test bug - source should be valid):\n{}",
            errors.join("\n")
        );
    }
    typed_ast
}

/// Borrow check phase - type checks first and panics on parse/type errors
/// Returns borrow errors for assertion
pub fn test_borrow_check(source: &str) -> Vec<BorrowError> {
    let typed_ast = type_check_or_panic(source);
    let mut checker = BorrowChecker::new(&typed_ast);
    checker.check_program()
}

/// Borrow check and panic on errors - for use in codegen phase
pub fn borrow_check_or_panic(source: &str) -> TypedAst {
    let typed_ast = type_check_or_panic(source);
    let mut checker = BorrowChecker::new(&typed_ast);
    let errors = checker.check_program();
    if !errors.is_empty() {
        let errors: Vec<String> = errors.iter().map(|e| format!("{:?}", e)).collect();
        panic!(
            "Borrow errors (test bug - source should be valid):\n{}",
            errors.join("\n")
        );
    }
    typed_ast
}

/// Full compilation and execution - panics on any compilation errors
/// Returns the exit code from running the compiled program
pub fn compile_and_run(source: Source) -> i32 {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let source = Arc::new(source);
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
        let errors: Vec<String> = diagnostics.iter().map(|d| format!("{}", d)).collect();
        panic!("Compilation errors:\n{}", errors.join("\n\n"));
    }

    let codegen = Codegen::new(&typed_ast, Triple::host())
        .unwrap_or_else(|diagnostic| panic!("Codegen error:\n{}", diagnostic));

    let product = codegen
        .compile()
        .unwrap_or_else(|diagnostic| panic!("Compile error:\n{}", diagnostic));

    let (libraries, needs_libc) = typed_ast.ast.get_extern_libraries();

    let library_paths = if cfg!(target_arch = "aarch64") {
        vec!["/opt/homebrew/lib".to_string()]
    } else {
        vec!["/usr/local/lib".to_string()]
    };

    let runtime_library_paths: Vec<_> = libraries
        .iter()
        .filter_map(|lib| Path::new(lib).parent())
        .map(|p| p.to_path_buf())
        .collect();

    let temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.into_temp_path();

    let linker = Linker::new(temp_path.to_str().unwrap())
        .with_libraries(libraries, needs_libc)
        .with_library_paths(library_paths);
    let executable = linker
        .link_object(product)
        .unwrap_or_else(|diagnostic| panic!("Linker error:\n{}", diagnostic));

    let runner = Runner::new(executable).with_library_paths(runtime_library_paths);
    let status = runner
        .run()
        .unwrap_or_else(|diagnostic| panic!("Runner error:\n{}", diagnostic));

    status.code().unwrap()
}

// --- Error predicate helpers ---

/// Check if any type error matches the predicate
pub fn has_type_error<F>(typed_ast: &TypedAst, predicate: F) -> bool
where
    F: Fn(&TypeError) -> bool,
{
    typed_ast.errors.iter().any(|e| predicate(e))
}

/// Check if any borrow error matches the predicate
pub fn has_borrow_error<F>(errors: &[BorrowError], predicate: F) -> bool
where
    F: Fn(&BorrowError) -> bool,
{
    errors.iter().any(predicate)
}
