use som_ast::Ast;
use som_common::DiagnosticSink;

mod constraint;
mod context;
mod hir;
mod pretty;
mod ty;
mod typer;

pub use constraint::*;
pub use context::*;
pub use hir::*;
pub use pretty::HirCtx;
pub use ty::*;
use typer::Typer;

pub use som_ast::{BinaryOp, UnaryOp};

pub fn typeck(ast: &Ast, diags: &mut DiagnosticSink) -> (Hir, TyCtx) {
    Typer::new().lower(ast, diags)
}
