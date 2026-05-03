use som_compiler::CompileResult;

pub fn compile(source: &str) -> CompileResult {
    som::init_tracing();
    som_compiler::compile(&som::Source::from_raw(source))
}
