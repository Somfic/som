use som::CompileResult;

pub fn compile(source: &str) -> CompileResult {
    som::init_tracing();
    som::compile(&som::Source::from_raw(source))
}
