pub use som_common::*;

pub struct CompileResult {
    pub diagnostics: Vec<Diagnostic>,
}

impl CompileResult {
    pub fn failed(diags: DiagnosticSink) -> Self {
        Self {
            diagnostics: diags.diagnostics().to_vec(),
        }
    }
}

pub fn compile(source: &Source) -> CompileResult {
    let mut sources = SourceMap::new();
    let id = sources.add(source.clone());
    let mut diags = DiagnosticSink::new();

    let content = sources.source(id).content();
    let ast = som_ast::parse(id, content, &mut diags);
    if std::env::var("SOM_DUMP_AST").is_ok() {
        info!("ast dump:\n{ast:#?}");
    }
    if diags.has_errors() {
        for diag in diags.diagnostics() {
            error!("{diag:#?}");
        }
        error!(
            "compilation failed with {} errors and {} warnings",
            diags.error_count(),
            diags.warning_count()
        );
        return CompileResult::failed(diags);
    }

    CompileResult {
        diagnostics: vec![],
    }
}

pub fn compile_and_run(source: &str) -> Result<i32, Vec<Diagnostic>> {
    let result = compile(&Source::from_raw(source));
    if !result.diagnostics.is_empty() {
        return Err(result.diagnostics);
    }
    // TODO: jit + run
    Ok(0)
}
