use som_ast::Ast;
use som_common::{DiagnosticSink, Id};

use crate::{BinaryOp, Constraint, Expr, Hir, Provenance, Stmt, TyCtx, Type, UnaryOp};

type UntypedExpr = som_ast::Expr;
type UntypedStmt = som_ast::Stmt;

pub(crate) struct Typer {
    ctx: TyCtx,
    ast: Hir,
    constraints: Vec<Constraint>,
}

impl Typer {
    pub(crate) fn new() -> Self {
        Self {
            ctx: TyCtx::new(),
            ast: Hir::new(),
            constraints: Vec::new(),
        }
    }

    pub(crate) fn lower(mut self, ast: &Ast, diags: &mut DiagnosticSink) -> (Hir, TyCtx) {
        for stmt_id in &ast.root {
            let stmt = self.infer_stmt(ast, *stmt_id);
            self.ast.root.push(stmt);
        }
        self.solve(diags);
        self.ctx.resolve_all();
        (self.ast, self.ctx)
    }

    fn infer(&mut self, ast: &Ast, id: Id<UntypedExpr>) -> Id<Expr> {
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
                let operand = self.infer(ast, operand);
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
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Unary(node),
                    expected: ty,
                    actual: operand_ty,
                });
                node
            }
            UntypedExpr::Binary { op, lhs, rhs, span } => {
                let lhs = self.infer(ast, lhs);
                let rhs = self.infer(ast, rhs);
                let lhs_ty = self.ast.get_expr(lhs).ty();
                let rhs_ty = self.ast.get_expr(rhs).ty();

                let (operand_ty, result_ty) = match op {
                    // int, int -> int
                    BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply | BinaryOp::Divide => {
                        let t = self.ctx.int(span);
                        (t, t)
                    }
                    // equality: T, T -> bool  (operands any type, but equal to each other)
                    BinaryOp::Equals | BinaryOp::NotEquals => {
                        (self.ctx.var(span), self.ctx.bool(span))
                    }
                    // ordering: int, int -> bool
                    BinaryOp::LessThan
                    | BinaryOp::LessThanOrEquals
                    | BinaryOp::GreaterThan
                    | BinaryOp::GreaterThanOrEquals => (self.ctx.int(span), self.ctx.bool(span)),
                    // bool, bool -> bool
                    BinaryOp::And | BinaryOp::Or => {
                        let t = self.ctx.bool(span);
                        (t, t)
                    }
                };

                let node = self.ast.add_expr(Expr::Binary {
                    op,
                    lhs,
                    rhs,
                    ty: result_ty,
                    span,
                });

                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::BinaryOp(node),
                    expected: operand_ty,
                    actual: lhs_ty,
                });

                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::BinaryOp(node),
                    expected: operand_ty,
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

    #[allow(dead_code)]
    fn check(&mut self, ast: &Ast, id: Id<UntypedExpr>, expected: Id<Type>) -> Id<Expr> {
        let node = self.infer(ast, id);
        let actual = self.ast.get_expr(node).ty();
        self.constraints.push(Constraint::Equal {
            provenance: Provenance::Check(node),
            expected,
            actual,
        });
        node
    }

    fn infer_stmt(&mut self, ast: &Ast, id: Id<UntypedStmt>) -> Id<Stmt> {
        match ast[id] {
            som_ast::Stmt::Expr { expr, span } => {
                let expr = self.infer(ast, expr);
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
