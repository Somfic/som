use som_common::{Id, LineWriter, Pretty, Show, SourceMap};

use crate::{Ast, Expr, Stmt};

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
        for stmt_id in &self.root {
            let stmt = &self[*stmt_id];
            let span = stmt.span();
            let mut buf = String::new();
            fmt_stmt(&mut buf, self, *stmt_id);
            buf.push(';');
            w.line(Some(span), 0, buf)?;
        }
        Ok(())
    }
}

fn fmt_stmt(buf: &mut String, ast: &Ast, id: Id<Stmt>) {
    match &ast[id] {
        Stmt::Expr { expr, .. } => fmt_expr(buf, ast, *expr, false),
        Stmt::Let { ident, expr, .. } => {
            buf.push_str("let ");
            buf.push_str(&ident);
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
        Expr::Block { span, stmts, value } => {
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
    }
}
