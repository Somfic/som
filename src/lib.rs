pub use som_common::*;

#[derive(Debug, Default, Clone, Copy)]
pub struct EmitSet {
    pub ast: bool,
    pub hir: bool,
    pub mir: bool,
    pub spans: bool,
}

pub struct CompileOptions {
    pub input: Source,
    pub emit: EmitSet,
    pub run: bool,
    pub opt_level: u8,
}

impl CompileOptions {
    pub fn new(input: Source) -> Self {
        Self {
            input,
            emit: EmitSet::default(),
            run: true,
            opt_level: 0,
        }
    }
}

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

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

pub fn compile(args: &CompileOptions) -> CompileResult<i64> {
    let mut sources = SourceMap::new();
    let id = sources.add(args.input.clone());
    let mut diags = DiagnosticSink::new();
    let content = sources.source(id).content();

    let ast = som_ast::parse(id, content, &mut diags);
    if args.emit.ast {
        if args.emit.spans {
            println!("{}", ast.display_with_sources(&sources));
        } else {
            println!("{}", ast.display());
        }
    }
    if diags.has_errors() {
        return CompileResult::failed(diags);
    }

    let (hir, tcx) = som_hir::typeck(&ast, &mut diags);
    if args.emit.hir {
        if args.emit.spans {
            println!("{}", hir.display_with_sources(&tcx, &sources));
        } else {
            println!("{}", hir.display(&tcx));
        }
    }
    if diags.has_errors() {
        return CompileResult::failed(diags);
    }

    let mir = som_mir::build(&hir, &tcx, &mut diags);
    if args.emit.mir {
        if args.emit.spans {
            println!("{}", mir.display_with_sources(&tcx, &sources));
        } else {
            println!("{}", mir.display(&tcx));
        }
    }
    if diags.has_errors() {
        return CompileResult::failed(diags);
    }

    if !args.run {
        return CompileResult {
            artifact: None,
            diagnostics: diags.finalize(),
        };
    }

    let func = match som_codegen::codegen(&mir, &tcx, args.opt_level) {
        Ok(f) => f,
        Err(e) => {
            let span = Span::from_range(id, 0..content.len());
            diags.emit_error(span, format!("codegen error: {e}"));
            return CompileResult::failed(diags);
        }
    };
    let result = func() as i64;
    CompileResult::success(diags, result)
}
