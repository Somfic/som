use som_common::{Arena, DiagnosticSink, Id, Span};
use som_hir::{Expr, Hir, Stmt, TyCtx};

use crate::{Block, Const, Function, LocalDecl, Operand, Rvalue, Statement, Terminator};

pub fn build(hir: &Hir, _tcx: &TyCtx, _diags: &mut DiagnosticSink) -> Function {
    let mut builder = MirBuilder::new(hir);

    let mut last_value: Option<Id<LocalDecl>> = None;
    for stmt_id in &hir.root {
        last_value = builder.lower_stmt(*stmt_id);
    }

    builder.func.return_local = last_value;
    builder.terminate(Terminator::Return);

    builder.finish()
}

struct MirBuilder<'a> {
    hir: &'a Hir,
    func: Function,
    current_block: Option<Id<Block>>,
}

impl<'a> MirBuilder<'a> {
    fn new(hir: &'a Hir) -> Self {
        let mut func = Function {
            locals: Arena::new(),
            blocks: Arena::new(),
            statements: Arena::new(),
            entry: Id::new(0),
            return_local: None,
        };
        let entry = func.new_block("entry");
        func.entry = entry;
        Self {
            hir,
            func,
            current_block: Some(entry),
        }
    }

    fn finish(self) -> Function {
        self.func
    }

    fn lower_stmt(&mut self, stmt_id: Id<Stmt>) -> Option<Id<LocalDecl>> {
        match self.hir.get_stmt(stmt_id) {
            Stmt::Expr { expr, .. } => Some(self.lower_expr(*expr)),
            Stmt::Let { ident, expr, span } => {
                let ty = self.hir.get_expr(*expr).ty();
                let local = self.func.alloc_local(ty, *span, ident.clone());
                let value = self.lower_expr(*expr);
                self.push_assign(local, Rvalue::Use(Operand::Copy(value)), *span);
                Some(local)
            }
            Stmt::Error { .. } => unreachable!("error stmt should not reach MIR"),
        }
    }

    fn lower_expr(&mut self, expr: Id<Expr>) -> Id<LocalDecl> {
        let expr = self.hir.get_expr(expr);
        match expr {
            Expr::Int { value, ty, span } => {
                let local = self.func.alloc_local(*ty, *span, "const");
                self.push_assign(
                    local,
                    Rvalue::Use(Operand::Const(Const::Int(*value, *ty))),
                    *span,
                );
                local
            }
            Expr::Bool { value, ty, span } => {
                let local = self.func.alloc_local(*ty, *span, "const");
                self.push_assign(
                    local,
                    Rvalue::Use(Operand::Const(Const::Bool(*value, *ty))),
                    *span,
                );
                local
            }
            Expr::Unary {
                span,
                ty,
                op,
                operand,
            } => {
                let operand_local = self.lower_expr(*operand);
                let result = self.func.alloc_local(*ty, *span, "unop");
                self.push_assign(
                    result,
                    Rvalue::UnaryOp(*op, Operand::Copy(operand_local)),
                    *span,
                );
                result
            }
            Expr::Binary {
                lhs,
                op,
                rhs,
                ty,
                span,
            } => {
                let lhs_local = self.lower_expr(*lhs);
                let rhs_local = self.lower_expr(*rhs);
                let result = self.func.alloc_local(*ty, *span, "binop");
                self.push_assign(
                    result,
                    Rvalue::BinaryOp(Operand::Copy(lhs_local), *op, Operand::Copy(rhs_local)),
                    *span,
                );
                result
            }
            Expr::Condition {
                span,
                ty,
                condition,
                truthy,
                falsy,
            } => {
                let condition_local = self.lower_expr(*condition);

                let result = self.func.alloc_local(*ty, *span, "cond.result");

                let truthy_block = self.func.new_block("cond.truthy");
                let falsy_block = self.func.new_block("cond.falsy");
                let merge_block = self.func.new_block("cond.merge");

                self.terminate(Terminator::SwitchInt {
                    discr: Operand::Copy(condition_local),
                    targets: vec![(1, truthy_block), (0, falsy_block)],
                });

                self.current_block = Some(truthy_block);
                let truthy_local = self.lower_expr(*truthy);
                self.push_assign(result, Rvalue::Use(Operand::Copy(truthy_local)), *span);
                self.terminate(Terminator::Goto(merge_block));

                self.current_block = Some(falsy_block);
                let falsy_local = self.lower_expr(*falsy);
                self.push_assign(result, Rvalue::Use(Operand::Copy(falsy_local)), *span);
                self.terminate(Terminator::Goto(merge_block));

                self.current_block = Some(merge_block);
                result
            }
            Expr::Block {
                stmts,
                value,
                ty,
                span,
            } => {
                // Copy out first: iterating `stmts` (borrowed from `self.hir`) while
                // calling `&mut self` methods below would otherwise conflict.
                let stmts = stmts.clone();
                let (value, ty, span) = (*value, *ty, *span);

                for stmt_id in stmts {
                    self.lower_stmt(stmt_id);
                }

                match value {
                    // The block's value is its trailing expression.
                    Some(expr) => self.lower_expr(expr),
                    // No tail expression → no value. Synthesise a placeholder until
                    // there's a real unit type.
                    None => {
                        let local = self.func.alloc_local(ty, span, "unit");
                        self.push_assign(
                            local,
                            Rvalue::Use(Operand::Const(Const::Int(0, ty))),
                            span,
                        );
                        local
                    }
                }
            }
            Expr::Error { .. } => unreachable!("error expr should not reach MIR"),
        }
    }

    fn push_assign(&mut self, local: Id<LocalDecl>, rvalue: Rvalue, span: Span) {
        let block_id = self.current_block.expect("no current block");
        let stmt = self.func.add_stmt(Statement::Assign {
            local,
            rvalue,
            span,
        });
        self.func.blocks.get_mut(&block_id).stmts.push(stmt);
    }

    fn terminate(&mut self, terminator: Terminator) {
        let block_id = self.current_block.take().expect("no current block");
        self.func.blocks.get_mut(&block_id).terminator = terminator;
    }
}
