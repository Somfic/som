use std::ops::Index;

use som_ast::Ast;
use som_common::{Arena, DiagnosticSink, Id, Span, info};

mod hir;
pub use hir::*;

pub use som_ast::BinaryOp;

mod ty;
pub use ty::*;

type UntypedExpr = som_ast::Expr;
type UntypedStmt = som_ast::Stmt;

pub fn typeck(ast: &Ast, diags: &mut DiagnosticSink) -> (Hir, TyCtx) {
    let typer = Typer::new();
    typer.lower(ast, diags)
}

pub struct Typer {
    ctx: TyCtx,
    ast: Hir,
}

impl Typer {
    pub fn new() -> Self {
        Self {
            ctx: TyCtx::new(),
            ast: Hir::new(),
        }
    }

    pub fn lower(mut self, ast: &Ast, diags: &mut DiagnosticSink) -> (Hir, TyCtx) {
        for stmt_id in &ast.root {
            let stmt = self.lower_stmt(ast, *stmt_id, diags);
            self.ast.root.push(stmt);
        }
        (self.ast, self.ctx)
    }

    fn lower_expr(
        &mut self,
        ast: &Ast,
        id: Id<UntypedExpr>,
        diags: &mut DiagnosticSink,
    ) -> Id<Expr> {
        match ast[id] {
            UntypedExpr::Int { value, span } => self.ast.add_expr(Expr::Int {
                value: value,
                ty: self.ctx.i32(span),
                span: span,
            }),
            UntypedExpr::Binary { op, lhs, rhs, span } => {
                let lhs = self.lower_expr(ast, lhs, diags);
                let rhs = self.lower_expr(ast, rhs, diags);
                // The "type checker": both i32 → i32. That's the whole rule for now.
                self.ast.add_expr(Expr::Binary {
                    op: op,
                    lhs,
                    rhs,
                    ty: self.ctx.i32(span),
                    span: span,
                })
            }
            UntypedExpr::Error { span } => self.ast.add_expr(Expr::Error {
                ty: self.ctx.error(span),
                span: span,
            }),
        }
    }

    fn lower_stmt(
        &mut self,
        ast: &Ast,
        id: Id<UntypedStmt>,
        diags: &mut DiagnosticSink,
    ) -> Id<Stmt> {
        match ast[id] {
            som_ast::Stmt::Expr { expr, span } => {
                let expr = self.lower_expr(ast, expr, diags);
                self.ast.add_stmt(Stmt::Expr { expr, span: span })
            }
        }
    }
}

#[derive(Debug)]
pub struct TyCtx {
    types: Arena<Type>,
}

impl Index<Id<Type>> for TyCtx {
    type Output = Type;
    fn index(&self, id: Id<Type>) -> &Type {
        self.types.get(&id)
    }
}

impl TyCtx {
    pub fn new() -> Self {
        Self {
            types: Arena::new(),
        }
    }

    pub fn error(&mut self, span: Span) -> Id<Type> {
        self.types.alloc(Type::Error { span })
    }

    pub fn i32(&mut self, span: Span) -> Id<Type> {
        self.types.alloc(Type::Int { span })
    }
}
