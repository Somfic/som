use som_common::{Id, LineWriter, Pretty, Show, SourceMap};

use crate::{Ast, Expr, Layout, Root, Stmt, TextPart, Ty};

#[derive(Copy, Clone)]
pub struct AstCtx<'a> {
    pub sources: Option<&'a SourceMap>,
}

impl Ast {
    pub fn display<'a>(&'a self) -> Show<'a, Ast, AstCtx<'a>> {
        Show::new(self, AstCtx { sources: None })
    }

    pub fn display_with_sources<'a>(&'a self, sources: &'a SourceMap) -> Show<'a, Ast, AstCtx<'a>> {
        Show::new(
            self,
            AstCtx {
                sources: Some(sources),
            },
        )
    }
}

impl Pretty<AstCtx<'_>> for Ast {
    fn pretty(&self, ctx: AstCtx<'_>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut w = LineWriter::new(f, ctx.sources);
        for root in &self.root {
            match *root {
                Root::Stmt(stmt_id) => {
                    let stmt = &self[stmt_id];
                    let span = stmt.span();
                    let mut buf = String::new();
                    fmt_stmt(&mut buf, self, stmt_id);
                    buf.push(';');
                    w.line(Some(span), 0, buf)?;
                }
                Root::Layout(layout_id) => fmt_layout(&mut w, self, layout_id, 0)?,
            }
        }
        Ok(())
    }
}

fn fmt_layout(
    w: &mut LineWriter<'_, '_>,
    ast: &Ast,
    id: Id<Layout>,
    depth: usize,
) -> std::fmt::Result {
    use std::fmt::Write;
    match &ast[id] {
        Layout::Element {
            tag,
            events,
            attr,
            children,
            span,
        } => {
            let mut buf = tag.to_string();
            for (name, value) in attr {
                let _ = write!(buf, " {name}=");
                fmt_expr(&mut buf, ast, *value, true);
            }
            for (name, body) in events {
                let _ = write!(buf, " @{name}: ");
                fmt_expr(&mut buf, ast, *body, true);
            }
            w.line(Some(*span), depth, buf)?;
            for child in children {
                fmt_layout(w, ast, *child, depth + 1)?;
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
                        fmt_expr(&mut buf, ast, *value, false);
                        buf.push('}');
                    }
                }
            }
            w.line(Some(*span), depth, format!("\"{buf}\""))
        }
    }
}

fn fmt_stmt(buf: &mut String, ast: &Ast, id: Id<Stmt>) {
    match &ast[id] {
        Stmt::Expr { expr, .. } => fmt_expr(buf, ast, *expr, false),
        Stmt::Let {
            ident, ty, expr, ..
        } => {
            buf.push_str("let ");
            buf.push_str(ident);
            if let Some(ty) = ty {
                buf.push_str(": ");
                buf.push_str(match ty {
                    Ty::I32 { .. } => "i32",
                    Ty::Bool { .. } => "bool",
                    Ty::Error { .. } => "<error>",
                });
            }
            buf.push_str(" = ");
            fmt_expr(buf, ast, *expr, false);
        }
    }
}

fn fmt_expr(buf: &mut String, ast: &Ast, id: Id<Expr>, nested: bool) {
    use std::fmt::Write;
    match &ast[id] {
        Expr::Error { .. } => {
            let _ = buf.write_str("<error>");
        }
        Expr::Int { value, .. } => {
            let _ = write!(buf, "{value}");
        }
        Expr::Bool { value, .. } => {
            let _ = write!(buf, "{value}");
        }
        Expr::Str { value, .. } => {
            let _ = write!(buf, "{value:?}");
        }
        Expr::Variable { name, .. } => {
            let _ = buf.write_str(name);
        }
        Expr::Unary { op, operand, .. } => {
            let _ = write!(buf, "{op}");
            fmt_expr(buf, ast, *operand, true);
        }
        Expr::Binary { lhs, op, rhs, .. } => {
            if nested {
                buf.push('(');
            }
            fmt_expr(buf, ast, *lhs, true);
            let _ = write!(buf, " {op} ");
            fmt_expr(buf, ast, *rhs, true);
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
            fmt_expr(buf, ast, *truthy, false);
            let _ = buf.write_str(" if ");
            fmt_expr(buf, ast, *condition, false);
            let _ = buf.write_str(" else ");
            fmt_expr(buf, ast, *falsy, false);
        }
        Expr::Block { stmts, value, .. } => {
            buf.push_str("{\n");
            for stmt in stmts {
                let mut stmt_buf = String::new();
                fmt_stmt(&mut stmt_buf, ast, *stmt);
                let _ = writeln!(buf, "    {stmt_buf}");
            }
            if let Some(value) = value {
                let mut value_buf = String::new();
                fmt_expr(&mut value_buf, ast, *value, false);
                let _ = writeln!(buf, "    {value_buf}");
            }
            buf.push('}');
        }
        Expr::Assignment { target, value, .. } => {
            let _ = buf.write_str(target);
            let _ = buf.write_str(" = ");
            fmt_expr(buf, ast, *value, nested);
        }
    }
}
