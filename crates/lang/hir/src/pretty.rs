use std::fmt;

use som_common::{Id, LineWriter, Pretty, Show, SourceMap};

use crate::{Expr, Hir, Layout, Root, Stmt, TextPart, TyCtx};

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
        for root in &self.root {
            match root {
                Root::Stmt(stmt_id) => {
                    let span = self.get_stmt(*stmt_id).span();
                    let mut buf = String::new();
                    fmt_stmt(&mut buf, self, ctx.tcx, *stmt_id);
                    buf.push(';');
                    w.line(Some(span), 0, buf)?;
                }
                Root::Layout(layout_id) => fmt_layout(&mut w, self, ctx.tcx, *layout_id, 0)?,
            }
        }
        Ok(())
    }
}

fn fmt_layout(
    w: &mut LineWriter<'_, '_>,
    hir: &Hir,
    tcx: &TyCtx,
    id: Id<Layout>,
    depth: usize,
) -> fmt::Result {
    use std::fmt::Write;
    match hir.get_layout(id) {
        Layout::Element {
            tag,
            events,
            attr,
            children,
            span,
        } => {
            let mut buf = tag.to_string();
            for (name, &value) in attr {
                let _ = write!(buf, " {name}=");
                fmt_expr(&mut buf, hir, tcx, value, true);
            }
            for (name, &body) in events {
                let _ = write!(buf, " @{name}: ");
                fmt_expr(&mut buf, hir, tcx, body, true);
            }
            w.line(Some(*span), depth, buf)?;
            for &child in children {
                fmt_layout(w, hir, tcx, child, depth + 1)?;
            }
            Ok(())
        }
        Layout::Text { text, span } => {
            let mut buf = String::new();
            for part in text {
                match part {
                    TextPart::Str { text, .. } => buf.push_str(text),
                    TextPart::Interp { value, .. } => {
                        buf.push('{');
                        fmt_expr(&mut buf, hir, tcx, *value, false);
                        buf.push('}');
                    }
                }
            }
            w.line(Some(*span), depth, format!("\"{buf}\""))
        }
    }
}

fn fmt_stmt(buf: &mut String, hir: &Hir, tcx: &TyCtx, id: Id<Stmt>) {
    use std::fmt::Write;
    match hir.get_stmt(id) {
        Stmt::Expr { expr, .. } => fmt_expr(buf, hir, tcx, *expr, false),
        Stmt::Let { ident, expr, .. } => {
            buf.push_str("let ");
            buf.push_str(ident);
            buf.push_str(" = ");
            fmt_expr(buf, hir, tcx, *expr, false);
        }
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
        Expr::Variable { name, ty, .. } => {
            let _ = write!(buf, "{name}: {}", tcx[*ty]);
        }
        Expr::Assignment {
            target, value, ty, ..
        } => {
            let _ = write!(buf, "{target} = ");
            fmt_expr(buf, hir, tcx, *value, true);
            let _ = write!(buf, ": {}", tcx[*ty]);
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
        Expr::Block { stmts, value, .. } => {
            buf.push_str("{\n");
            for stmt_id in stmts {
                let mut line = String::new();
                fmt_stmt(&mut line, hir, tcx, *stmt_id);
                let _ = writeln!(buf, "    {line};");
            }
            if let Some(value) = value {
                let mut line = String::new();
                fmt_expr(&mut line, hir, tcx, *value, false);
                let _ = writeln!(buf, "    {line}");
            }
            buf.push('}');
        }
    }
}
