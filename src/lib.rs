pub use som_common::*;

mod render;
pub use render::render_diagnostic;

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
    /// The sources the diagnostics point into, kept so callers can render
    /// spans with source context.
    pub sources: SourceMap,
}

impl<T> CompileResult<T> {
    pub fn success(sources: SourceMap, diags: DiagnosticSink, artifact: T) -> Self {
        Self {
            artifact: Some(artifact),
            diagnostics: diags.finalize(),
            sources,
        }
    }

    pub fn failed(sources: SourceMap, diags: DiagnosticSink) -> Self {
        Self {
            artifact: None,
            diagnostics: diags.finalize(),
            sources,
        }
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// What a successful compile produced: a computed value (pure logic, via
/// Cranelift) or a UI program to run in a window (via the walker).
pub enum Outcome {
    Value(i64),
    Ui(som_hir::Hir),
}

/// Open a window and run a UI program's reactive event loop. Blocks until the
/// window closes.
pub fn run_ui(hir: som_hir::Hir) {
    use std::rc::Rc;
    let hir = Rc::new(hir);
    som_canvas_blitz::run(move |body| som_eval::build_ui(hir, body));
}

pub fn compile(args: &CompileOptions) -> CompileResult<Outcome> {
    let mut sources = SourceMap::new();
    let id = sources.add(args.input.clone());
    let mut diags = DiagnosticSink::new();
    // Own the content so `sources` stays free to move into the result.
    let content = sources.source(id).content().to_string();

    let ast = som_ast::parse(id, &content, &mut diags);
    if args.emit.ast {
        if args.emit.spans {
            println!("{}", ast.display_with_sources(&sources));
        } else {
            println!("{}", ast.display());
        }
    }
    if diags.has_errors() {
        return CompileResult::failed(sources, diags);
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
        return CompileResult::failed(sources, diags);
    }

    // A program with layout is a UI: it runs through the walker in a window,
    // not through MIR/Cranelift (which only handles pure logic).
    if hir.root.iter().any(|r| matches!(r, som_hir::Root::Layout(_))) {
        if !args.run {
            return CompileResult::failed(sources, diags);
        }
        return CompileResult::success(sources, diags, Outcome::Ui(hir));
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
        return CompileResult::failed(sources, diags);
    }

    if !args.run {
        return CompileResult::failed(sources, diags);
    }

    let func = match som_codegen::codegen(&mir, &tcx, args.opt_level) {
        Ok(f) => f,
        Err(e) => {
            let span = Span::from_range(id, 0..content.len());
            diags.emit_error(span, format!("codegen error: {e}"));
            return CompileResult::failed(sources, diags);
        }
    };
    let result = func() as i64;
    CompileResult::success(sources, diags, Outcome::Value(result))
}
