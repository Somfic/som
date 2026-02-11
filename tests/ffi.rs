use som::{
    BorrowChecker, Codegen, Diagnostic, Linker, Runner, Source, TypeInferencer, parser, source_raw,
};
use std::path::Path;
use std::sync::Arc;
use target_lexicon::Triple;

fn run(source: Source) -> i32 {
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

    // Add common library search paths (Homebrew)
    let library_paths = if cfg!(target_arch = "aarch64") {
        vec!["/opt/homebrew/lib".to_string()]
    } else {
        vec!["/usr/local/lib".to_string()]
    };

    // Extract directories from library paths for runtime lookup
    let runtime_library_paths: Vec<_> = libraries
        .iter()
        .filter_map(|lib| Path::new(lib).parent())
        .map(|p| p.to_path_buf())
        .collect();

    let linker = Linker::new("test_ffi")
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

#[test]
fn identity() {
    let source = source_raw!(
        r#"
    extern "test_ffi.so" {
        fn identity(x: i32) -> i32;
    }

    fn main() -> i32 {
        identity(123)
    }
    "#
    );
    let code = run(source);
    assert_eq!(code, 123);
}
