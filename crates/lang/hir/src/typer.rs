use som_ast::Ast;
use som_common::{Diagnostic, DiagnosticSink, Id, Scope, code, message};

use crate::{BinaryOp, Binding, Constraint, Expr, Hir, Provenance, Stmt, TyCtx, Type, UnaryOp};

type UntypedExpr = som_ast::Expr;
type UntypedStmt = som_ast::Stmt;

pub(crate) struct Typer {
    ctx: TyCtx,
    ast: Hir,
    constraints: Vec<Constraint>,
    scope: Scope<Id<Binding>>,
}

impl Typer {
    pub(crate) fn new() -> Self {
        Self {
            ctx: TyCtx::new(),
            ast: Hir::new(),
            constraints: Vec::new(),
            scope: Scope::new(),
        }
    }

    pub(crate) fn lower(mut self, ast: &Ast, diags: &mut DiagnosticSink) -> (Hir, TyCtx) {
        for stmt_id in &ast.root {
            let stmt = self.infer_stmt(ast, *stmt_id, diags);
            self.ast.root.push(stmt);
        }
        self.solve(diags);
        self.ctx.resolve_all();
        (self.ast, self.ctx)
    }

    fn infer(&mut self, ast: &Ast, id: Id<UntypedExpr>, diags: &mut DiagnosticSink) -> Id<Expr> {
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
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Unary(node),
                    expected: ty,
                    actual: operand_ty,
                });
                node
            }
            UntypedExpr::Binary { op, lhs, rhs, span } => {
                let (op, lhs, rhs, span) = (*op, *lhs, *rhs, *span);
                let lhs = self.infer(ast, lhs, diags);
                let rhs = self.infer(ast, rhs, diags);
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
            UntypedExpr::Variable { name, span } => {
                let span = *span;
                let binding = self.scope.lookup(name).copied();
                let ty = match binding {
                    Some(b) => self.ast.binding(b).ty,
                    None => {
                        diags.emit(
                            Diagnostic::error(span, message!["unknown variable ", code(name)])
                                .label("not found in this scope")
                                .note("variables must be introduced with `let` before use"),
                        );
                        self.ctx.error(span)
                    }
                };
                self.ast.add_expr(Expr::Variable {
                    name: name.clone(),
                    binding,
                    ty,
                    span,
                })
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
                let condition = self.infer(ast, condition, diags);
                let truthy = self.infer(ast, truthy, diags);
                let falsy = self.infer(ast, falsy, diags);

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

                let scope = self.scope.enter();
                let stmts = stmts
                    .iter()
                    .map(|&s| self.infer_stmt(ast, s, diags))
                    .collect();

                let value = value.as_ref().map(|&v| self.infer(ast, v, diags));
                self.scope.exit(scope);

                let ty = match value {
                    Some(v) => self.ast.get_expr(v).ty(),
                    None => self.ctx.nothing(span),
                };

                self.ast.add_expr(Expr::Block {
                    stmts,
                    value,
                    ty,
                    span,
                })
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

    /// Lower a syntactic type annotation to a concrete type.
    fn lower_ty(&mut self, ty: &som_ast::Ty) -> Id<Type> {
        match ty {
            som_ast::Ty::I32 { span } => self.ctx.i32(*span),
            som_ast::Ty::Bool { span } => self.ctx.bool(*span),
            som_ast::Ty::Error { span } => self.ctx.error(*span),
        }
    }

    fn infer_stmt(
        &mut self,
        ast: &Ast,
        id: Id<UntypedStmt>,
        diags: &mut DiagnosticSink,
    ) -> Id<Stmt> {
        match &ast[id] {
            som_ast::Stmt::Expr { expr, span } => {
                let expr = self.infer(ast, *expr, diags);
                self.ast.add_stmt(Stmt::Expr { expr, span: *span })
            }
            som_ast::Stmt::Let {
                span,
                ident,
                ty,
                expr,
            } => {
                // With an annotation we *check* the initialiser against the
                // declared type; without one we *infer* it. Either way the
                // binding records the resulting type.
                let (expr, ty) = match ty {
                    Some(ty) => {
                        let expected = self.lower_ty(ty);
                        (self.check(ast, *expr, expected, diags), expected)
                    }
                    None => {
                        let expr = self.infer(ast, *expr, diags);
                        (expr, self.ast.get_expr(expr).ty())
                    }
                };
                let binding = self.ast.add_binding(Binding {
                    name: ident.clone(),
                    span: *span,
                    ty,
                });
                self.scope.define(ident.clone(), binding);
                self.ast.add_stmt(Stmt::Let {
                    ident: ident.clone(),
                    binding,
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
                    message!["type mismatch: expected ", code(want), ", found ", code(got)],
                );
            }
        }
    }
}
