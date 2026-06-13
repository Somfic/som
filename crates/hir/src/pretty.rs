use std::fmt;

use som_common::{Id, LineWriter, Pretty, Show, SourceMap};

use crate::{Expr, Hir, Stmt, TyCtx};

#[derive(Copy, Clone)]
pub struct HirCtx<'a> {
    pub tcx: &'a TyCtx,
    pub sources: Option<&'a SourceMap>,
}

impl Hir {
    pub fn display<'a>(&'a self, tcx: &'a TyCtx) -> Show<'a, Hir, HirCtx<'a>> {
        Show::new(self, HirCtx { tcx, sources: None })
    }

    pub fn display_with_sources<'a>(
        &'a self,
        tcx: &'a TyCtx,
        sources: &'a SourceMap,
    ) -> Show<'a, Hir, HirCtx<'a>> {
        Show::new(
            self,
            HirCtx {
                tcx,
                sources: Some(sources),
            },
        )
    }
}

impl Pretty<HirCtx<'_>> for Hir {
    fn pretty(&self, ctx: HirCtx<'_>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut w = LineWriter::new(f, ctx.sources);
        for stmt_id in &self.root {
            let stmt = self.get_stmt(*stmt_id);
            let span = stmt.span();
            let mut buf = String::new();
            fmt_stmt(&mut buf, self, ctx.tcx, *stmt_id);
            buf.push(';');
            w.line(Some(span), 0, buf)?;
        }
        Ok(())
    }
}

fn fmt_stmt(buf: &mut String, hir: &Hir, tcx: &TyCtx, id: Id<Stmt>) {
    use std::fmt::Write;
    match hir.get_stmt(id) {
        Stmt::Expr { expr, .. } => fmt_expr(buf, hir, tcx, *expr, false),
        Stmt::Error { .. } => {
            let _ = buf.write_str("<error>");
        }
    }
}

fn fmt_expr(buf: &mut String, hir: &Hir, tcx: &TyCtx, id: Id<Expr>, nested: bool) {
    use std::fmt::Write;
    let expr = hir.get_expr(id);
    match expr {
        Expr::Error { ty, .. } => {
            let _ = write!(buf, "<error>: {}", tcx[*ty]);
        }
        Expr::Int { value, ty, .. } => {
            let _ = write!(buf, "{value}: {}", tcx[*ty]);
        }
        Expr::Bool { value, ty, .. } => {
            let _ = write!(buf, "{value}: {}", tcx[*ty]);
        }
        Expr::Unary { op, operand, .. } => {
            let _ = write!(buf, "{op}");
            fmt_expr(buf, hir, tcx, *operand, true);
        }
        Expr::Binary { lhs, op, rhs, .. } => {
            if nested {
                buf.push('(');
            }
            fmt_expr(buf, hir, tcx, *lhs, true);
            let _ = write!(buf, " {op} ");
            fmt_expr(buf, hir, tcx, *rhs, true);
            if nested {
                buf.push(')');
            }
        }
        Expr::Condition {
            condition,
            truthy,
            falsy,
            ..
        } => {
            fmt_expr(buf, hir, tcx, *truthy, false);
            let _ = write!(buf, " if ");
            fmt_expr(buf, hir, tcx, *condition, false);
            let _ = write!(buf, " else ");
            fmt_expr(buf, hir, tcx, *falsy, false);
        }
    }
}
