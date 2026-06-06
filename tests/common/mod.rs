use som::CompileResult;

pub fn compile(source: &str) -> CompileResult<i64> {
    som::init_tracing();
    som::compile(&som::Source::from_raw(source))
}

pub fn expect(source: &str, expected: i64) {
    som::init_tracing();
    let result = compile(source).artifact.expect("compilation to succeed");
    assert_eq!(result, expected);
}
