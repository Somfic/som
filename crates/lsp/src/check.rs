//! Check-only compile pipeline for the language server: parse, type-check, and
//! lower to MIR to surface every diagnostic, but never codegen or run. Mirrors
//! the driver in the root crate; kept here so the LSP crate doesn't depend on
//! the binary (which would be a dependency cycle).

use som_common::{Diagnostic, DiagnosticSink, Id, Source, SourceMap};

/// Compile `text` far enough to collect diagnostics, stopping at the first
/// stage that reports errors (later stages assume well-formed input).
///
/// Byte offsets in the returned diagnostics index into `text` directly, since
/// the whole program is this single in-memory source.
pub fn check(text: &str) -> Vec<Diagnostic> {
    let mut sources = SourceMap::new();
    let id: Id<Source> = sources.add(Source::from_raw(text.to_string()));
    let mut diags = DiagnosticSink::new();
    let content = sources.source(id).content().to_string();

    let ast = som_ast::parse(id, &content, &mut diags);
    if diags.has_errors() {
        return diags.finalize();
    }

    let (hir, tcx) = som_hir::typeck(&ast, &mut diags);
    if diags.has_errors() {
        return diags.finalize();
    }

    let _ = som_mir::build(&hir, &tcx, &mut diags);
    diags.finalize()
}
