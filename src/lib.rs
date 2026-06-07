pub use som_common::*;

pub struct CompileResult<T> {
    pub artifact: Option<T>,
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> CompileResult<T> {
    pub fn success(diags: DiagnosticSink, artifact: T) -> Self {
        Self {
            artifact: Some(artifact),
            diagnostics: diags.finalize(),
        }
    }

    pub fn failed(diags: DiagnosticSink) -> Self {
        Self {
            artifact: None,
            diagnostics: diags.finalize(),
        }
    }
}

pub fn compile(source: &Source) -> CompileResult<i64> {
    let mut sources = SourceMap::new();
    let id = sources.add(source.clone());
    let mut diags = DiagnosticSink::new();

    let dump_spans = std::env::var("SOM_DUMP_SPANS").is_ok();
    let content = sources.source(id).content();
    let ast = som_ast::parse(id, content, &mut diags);
    if std::env::var("SOM_DUMP_AST").is_ok() {
        if dump_spans {
            info!("AST dump:\n{}", ast.display_with_sources(&sources));
        } else {
            info!("AST dump:\n{}", ast.display());
        }
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

    let (hir, tcx) = som_hir::typeck(&ast, &mut diags);
    if std::env::var("SOM_DUMP_HIR").is_ok() {
        if dump_spans {
            info!("HIR dump:\n{}", hir.display_with_sources(&tcx, &sources));
        } else {
            info!("HIR dump:\n{}", hir.display(&tcx));
        }
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

    let mir = som_mir::build(&hir, &tcx, &mut diags);
    if std::env::var("SOM_DUMP_MIR").is_ok() {
        if dump_spans {
            info!("MIR dump:\n{}", mir.display_with_sources(&tcx, &sources));
        } else {
            info!("MIR dump:\n{}", mir.display(&tcx));
        }
    }
    if diags.has_errors() {
        return CompileResult::failed(diags);
    }

    let func = match som_codegen::codegen(&mir, &tcx) {
        Ok(f) => f,
        Err(e) => {
            error!("codegen failed: {e}");
            return CompileResult::failed(diags);
        }
    };
    let result = func() as i64;

    CompileResult {
        diagnostics: diags.finalize(),
        artifact: Some(result),
    }
}
