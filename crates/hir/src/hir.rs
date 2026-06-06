use std::fmt;

use som_ast::{BinaryOp, UnaryOp};
use som_common::{Arena, Id, LineWriter, Pretty, Show, SourceMap, Span, expand_enum};

use crate::{TyCtx, Type};

#[derive(Copy, Clone)]
pub struct HirCtx<'a> {
    pub tcx: &'a TyCtx,
    pub sources: Option<&'a SourceMap>,
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug)]
    pub enum Expr {
        Error,
        Int { value: i64 },
        Unary { op: UnaryOp, operand: Id<Expr> },
        Binary { lhs: Id<Expr>, op: BinaryOp, rhs: Id<Expr> },
    } with { span: Span, ty: Id<Type> }
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug)]
    pub enum Stmt {
        Error,
        Expr { expr: Id<Expr> },
    } with { span: Span }
}

#[derive(Debug)]
pub struct Hir {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    pub root: Vec<Id<Stmt>>,
}

impl Hir {
    pub fn new() -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            root: Vec::new(),
        }
    }

    pub fn add_expr(&mut self, expr: Expr) -> Id<Expr> {
        self.exprs.alloc(expr)
    }

    pub fn add_stmt(&mut self, stmt: Stmt) -> Id<Stmt> {
        self.stmts.alloc(stmt)
    }

    pub fn get_expr(&self, id: Id<Expr>) -> &Expr {
        self.exprs.get(&id)
    }

    pub fn get_stmt(&self, id: Id<Stmt>) -> &Stmt {
        self.stmts.get(&id)
    }

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

fn fmt_ty(ty: &Type) -> &'static str {
    match ty {
        Type::Int { .. } => "i32",
        Type::Error { .. } => "?",
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
            let _ = write!(buf, "<error>: {}", fmt_ty(&tcx[*ty]));
        }
        Expr::Int { value, ty, .. } => {
            let _ = write!(buf, "{value}: {}", fmt_ty(&tcx[*ty]));
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
    }
}
