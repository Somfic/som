use som::{CompileOptions, CompileResult, Source};

pub fn compile(source: &str) -> CompileResult<i64> {
    som::init_tracing();
    som::compile(&CompileOptions::new(Source::from_raw(source)))
}

pub fn expect(source: &str, expected: i64) {
    som::init_tracing();
    let compiled = compile(source);

    if compiled.has_errors() {
        panic!(
            "expected `{source}` to compile successfully, but got errors: {:?}",
            compiled.diagnostics
        );
    }

    assert_eq!(compiled.artifact.unwrap(), expected);
}

/// Assert that compilation fails with a type-mismatch diagnostic (and does not
/// panic downstream in MIR/codegen).
pub fn expect_type_error(source: &str) {
    som::init_tracing();
    let result = compile(source);
    assert!(
        result.artifact.is_none(),
        "expected `{source}` to fail type checking, but it compiled"
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("type mismatch")),
        "expected a type-mismatch diagnostic for `{source}`, got: {:?}",
        result.diagnostics
    );
}
