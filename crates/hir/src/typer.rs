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
        // `&ast[id]`: the AST `Expr` holds a `Vec` (Block) so it is no longer `Copy`.
        match &ast[id] {
            UntypedExpr::Int { value, span } => {
                let (value, span) = (*value, *span);
                let ty = self.ctx.int(span);
                self.ast.add_expr(Expr::Int { value, ty, span })
            }
            UntypedExpr::Bool { value, span } => {
                let (value, span) = (*value, *span);
                let ty = self.ctx.bool(span);
                self.ast.add_expr(Expr::Bool { value, ty, span })
            }
            UntypedExpr::Unary { op, operand, span } => {
                let (op, operand, span) = (*op, *operand, *span);
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
                let (op, lhs, rhs, span) = (*op, *lhs, *rhs, *span);
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
                let span = *span;
                let ty = self.ctx.error(span);
                self.ast.add_expr(Expr::Error { ty, span })
            }
            UntypedExpr::Condition {
                span,
                condition,
                truthy,
                falsy,
            } => {
                let (span, condition, truthy, falsy) = (*span, *condition, *truthy, *falsy);
                let condition = self.infer(ast, condition);
                let truthy = self.infer(ast, truthy);
                let falsy = self.infer(ast, falsy);

                let condition_ty = self.ast.get_expr(condition).ty();
                let truthy_ty = self.ast.get_expr(truthy).ty();
                let falsy_ty = self.ast.get_expr(falsy).ty();

                let result_ty = self.ctx.var(span);
                let bool_ty = self.ctx.bool(span);

                let node = self.ast.add_expr(Expr::Condition {
                    condition,
                    truthy,
                    falsy,
                    ty: result_ty,
                    span,
                });

                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Condition(node),
                    expected: bool_ty,
                    actual: condition_ty,
                });

                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Condition(node),
                    expected: result_ty,
                    actual: truthy_ty,
                });

                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Condition(node),
                    expected: result_ty,
                    actual: falsy_ty,
                });

                node
            }
            UntypedExpr::Block { stmts, value, span } => {
                let span = *span;
                // Statements are typed in order — this is where `let` bindings will
                // enter scope once variable references exist.
                let stmts: Vec<_> = stmts.iter().map(|&s| self.infer_stmt(ast, s)).collect();
                // The block's value (and type) is its trailing expression, if any.
                let value = value.as_ref().map(|&v| self.infer(ast, v));
                let ty = match value {
                    Some(v) => self.ast.get_expr(v).ty(),
                    // No tail expression → no value. Needs a real unit type later;
                    // for now treat it as the error type so it can't be used.
                    None => self.ctx.error(span),
                };
                self.ast.add_expr(Expr::Block { stmts, value, ty, span })
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
        match &ast[id] {
            som_ast::Stmt::Expr { expr, span } => {
                let expr = self.infer(ast, *expr);
                self.ast.add_stmt(Stmt::Expr { expr, span: *span })
            }
            som_ast::Stmt::Let { span, ident, expr } => {
                let expr = self.infer(ast, *expr);
                self.ast.add_stmt(Stmt::Let {
                    ident: ident.clone(),
                    expr,
                    span: *span,
                })
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
