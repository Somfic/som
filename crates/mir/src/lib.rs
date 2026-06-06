use som_common::{Arena, DiagnosticSink, Id, Span};
use som_hir::{Expr, Hir, Stmt, TyCtx};

mod graph;
pub use graph::*;

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
            Expr::Binary {
                lhs,
                op,
                rhs,
                ty,
                span,
            } => {
                let lhs_local = self.lower_expr(*lhs);
                let rhs_local = self.lower_expr(*rhs);
                let result = self.func.alloc_local(*ty, *span, binop_name(*op));
                self.push_assign(
                    result,
                    Rvalue::BinaryOp(Operand::Copy(lhs_local), *op, Operand::Copy(rhs_local)),
                    *span,
                );
                result
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

fn binop_name(op: som_hir::BinaryOp) -> &'static str {
    use som_hir::BinaryOp::*;
    match op {
        Add => "add",
        Subtract => "sub",
        Multiply => "mul",
        Divide => "div",
    }
}
