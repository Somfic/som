use som_ast::Ast;
use som_common::DiagnosticSink;

mod constraint;
mod context;
mod hir;
mod ty;
mod typer;

pub use constraint::*;
pub use context::*;
pub use hir::*;
pub use ty::*;
pub use typer::Typer;

pub use som_ast::{BinaryOp, UnaryOp};

pub fn typeck(ast: &Ast, diags: &mut DiagnosticSink) -> (Hir, TyCtx) {
    Typer::new().lower(ast, diags)
}
