use som_ast::Ast;
use som_common::{DiagnosticSink, Id};

use crate::{BinaryOp, Constraint, Expr, Hir, Provenance, Stmt, TyCtx, Type, UnaryOp};

type UntypedExpr = som_ast::Expr;
type UntypedStmt = som_ast::Stmt;

pub struct Typer {
    ctx: TyCtx,
    ast: Hir,
    constraints: Vec<Constraint>,
}

impl Typer {
    pub fn new() -> Self {
        Self {
            ctx: TyCtx::new(),
            ast: Hir::new(),
            constraints: Vec::new(),
        }
    }

    pub fn lower(mut self, ast: &Ast, diags: &mut DiagnosticSink) -> (Hir, TyCtx) {
        for stmt_id in &ast.root {
            let stmt = self.infer_stmt(ast, *stmt_id, diags);
            self.ast.root.push(stmt);
        }
        self.solve(diags);
        self.ctx.resolve_all();
        (self.ast, self.ctx)
    }

    fn infer(&mut self, ast: &Ast, id: Id<UntypedExpr>, diags: &mut DiagnosticSink) -> Id<Expr> {
        match ast[id] {
            UntypedExpr::Int { value, span } => {
                let ty = self.ctx.int(span);
                self.ast.add_expr(Expr::Int { value, ty, span })
            }
            UntypedExpr::Bool { value, span } => {
                let ty = self.ctx.bool(span);
                self.ast.add_expr(Expr::Bool { value, ty, span })
            }
            UntypedExpr::Unary { op, operand, span } => {
                let operand = self.infer(ast, operand, diags);
                let operand_ty = self.ast.get_expr(operand).ty();
                let ty = match op {
                    UnaryOp::Negate => self.ctx.int(span),
                    UnaryOp::Not => self.ctx.bool(span),
                };
                let node = self.ast.add_expr(Expr::Unary {
                    op,
                    operand,
                    ty,
                    span,
                });
                // the operand must have the same type as the result
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Unary(node),
                    expected: ty,
                    actual: operand_ty,
                });
                node
            }
            UntypedExpr::Binary { op, lhs, rhs, span } => {
                let lhs = self.infer(ast, lhs, diags);
                let rhs = self.infer(ast, rhs, diags);
                let lhs_ty = self.ast.get_expr(lhs).ty();
                let rhs_ty = self.ast.get_expr(rhs).ty();
                let ty = match op {
                    BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                        self.ctx.int(span)
                    }
                };
                let node = self.ast.add_expr(Expr::Binary {
                    op,
                    lhs,
                    rhs,
                    ty,
                    span,
                });
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::BinaryOp(node),
                    expected: ty,
                    actual: lhs_ty,
                });
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::BinaryOp(node),
                    expected: ty,
                    actual: rhs_ty,
                });
                node
            }
            UntypedExpr::Error { span } => {
                let ty = self.ctx.error(span);
                self.ast.add_expr(Expr::Error { ty, span })
            }
        }
    }

    fn check(
        &mut self,
        ast: &Ast,
        id: Id<UntypedExpr>,
        expected: Id<Type>,
        diags: &mut DiagnosticSink,
    ) -> Id<Expr> {
        let node = self.infer(ast, id, diags);
        let actual = self.ast.get_expr(node).ty();
        self.constraints.push(Constraint::Equal {
            provenance: Provenance::Check(node),
            expected,
            actual,
        });
        node
    }

    fn infer_stmt(
        &mut self,
        ast: &Ast,
        id: Id<UntypedStmt>,
        diags: &mut DiagnosticSink,
    ) -> Id<Stmt> {
        match ast[id] {
            som_ast::Stmt::Expr { expr, span } => {
                let expr = self.infer(ast, expr, diags);
                self.ast.add_stmt(Stmt::Expr { expr, span })
            }
        }
    }

    fn solve(&mut self, diags: &mut DiagnosticSink) {
        let constraints = std::mem::take(&mut self.constraints);
        for constraint in constraints {
            let Constraint::Equal {
                provenance,
                expected,
                actual,
            } = constraint;
            if self.ctx.unify(expected, actual).is_err() {
                let want = self.ctx.describe(expected);
                let got = self.ctx.describe(actual);
                let span = self.ast.get_expr(provenance.expr()).span();
                diags.emit_error(
                    span,
                    format!("type mismatch: expected `{want}`, found `{got}`"),
                );
            }
        }
    }
}
