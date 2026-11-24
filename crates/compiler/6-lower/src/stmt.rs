use som_ast::{Expr, Id, Ident, Span, Stmt};
use som_mir::*;

use super::FnBuilder;

impl FnBuilder<'_, '_> {
    pub(super) fn stmts(&mut self, stmts: &[Id<Stmt>], span: Span) {
        self.in_scope(span, |b| stmts.iter().for_each(|&s| b.stmt(s)));
    }

    pub(super) fn stmt(&mut self, id: Id<Stmt>) {
        let ast = self.l.ast;
        let span = ast.stmts.span(id).clone();
        let first_new = self.locals.slots();
        self.stmt_inner(id, span.clone());
        // Dropped, unless they were moved.
        let temps: Vec<Local> = (first_new..self.locals.slots())
            .map(Id::new)
            .filter(|&l| self.locals[l].name.is_none())
            .collect();
        for temp in temps {
            self.drop_local(temp, span.clone());
        }
    }

    fn stmt_inner(&mut self, id: Id<Stmt>, span: Span) {
        self.l.at = Some(span.clone());
        let ast = self.l.ast;
        match &ast.stmts[id] {
            // A constant is made where it's used, or worked out while compiling.
            Stmt::Use(_) | Stmt::Const(_) => {}
            Stmt::Let {
                name,
                mutable,
                value,
                ..
            } => self.let_stmt(id, name, *mutable, *value, span),
            // `@for f in T` over a generic type: the copy for this instance's `T`.
            Stmt::Expr(_) if let Some(arm) = self.typed.chosen_arm(id) => {
                self.operand(arm);
            }
            Stmt::Expr(expr) => self.expr_stmt(*expr, span),
            Stmt::Loop { body } => self.loop_stmt(body, span),
            Stmt::While { condition, body } => self.while_stmt(*condition, body, span),
            Stmt::For {
                name,
                mutable,
                iterable,
                body,
            } if ast.settled.contains(&id) => {
                self.settled_for(id, name, *mutable, *iterable, body, span)
            }
            Stmt::For {
                name,
                mutable,
                iterable,
                body,
            } => self.for_loop(id, name, *mutable, *iterable, body, span),
            Stmt::Return(value) => self.return_stmt(*value, span),
            Stmt::Break(named) | Stmt::Continue(named) => {
                let is_break = matches!(ast.stmts[id], Stmt::Break(_));
                self.break_or_continue(id, named.is_some(), is_break, span)
            }
            // `@if GPU`: the GPU's branch in its copy, the other in the CPU's.
            Stmt::If {
                then, otherwise, ..
            } if let Some(&then_gpu) = ast.gpu_ifs.get(&id) => {
                let branch = if self.l.gpu == then_gpu {
                    Some(then)
                } else {
                    otherwise.as_ref()
                };
                if let Some(branch) = branch {
                    self.stmts(branch, span);
                }
            }
            // `@if`: only the branch it picks.
            Stmt::If {
                then, otherwise, ..
            } if let Some(holds) = self.typed.settled_if(id) => {
                let branch = if holds {
                    Some(then)
                } else {
                    otherwise.as_ref()
                };
                if let Some(branch) = branch {
                    self.stmts(branch, span);
                }
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => self.if_stmt(*condition, then, otherwise.as_deref(), span),
        }
    }

    fn let_stmt(&mut self, id: Id<Stmt>, name: &Ident, mutable: bool, value: Id<Expr>, span: Span) {
        let value = self.rvalue(value);
        let def = self.l.names.local(id).expect("every let is resolved");
        let ty = self.typed.local(def);
        let ty = self.ty(ty);
        let local = self.let_local(def, name, ty, mutable, span.clone());
        self.assign(Place::local(local), value, span);
    }

    fn expr_stmt(&mut self, expr: Id<Expr>, span: Span) {
        // The result is unused: a temporary holding it dies with the statement.
        if let Operand::Copy(place) | Operand::Move(place) = self.operand(expr)
            && place.projection.is_empty()
            && place.local != RETURN
            && self.locals[place.local].name.is_none()
        {
            self.end_local(place.local, span);
        }
    }

    fn return_stmt(&mut self, value: Option<Id<Expr>>, span: Span) {
        let bare = value.is_none();
        let value = match value {
            Some(value) => self.operand(value),
            None => {
                let unit = self.l.mir.types.unit;
                self.constant(ConstKind::Unit, unit)
            }
        };
        // `return;` of a `-> !`: it gives `()`, held.
        let ret = self.locals[RETURN].ty;
        let given = match self.outcome_inner(ret) {
            Some(ok) if bare && ok == self.l.mir.types.unit => self.ok_wrapped(value, ok),
            _ => Rvalue::Use(value),
        };
        self.assign(Place::local(RETURN), given, span.clone());
        self.end_scopes_from(0, span.clone());
        for arg in (1..=self.param_count()).rev() {
            self.end_local(Id::new(arg), span.clone());
        }
        self.terminate(TerminatorKind::Return, span);
        // Whatever follows can't be reached.
        self.current = self.new_block();
    }

    fn if_stmt(
        &mut self,
        condition: Id<Expr>,
        then: &[Id<Stmt>],
        otherwise: Option<&[Id<Stmt>]>,
        span: Span,
    ) {
        let condition = self.condition(condition);
        let (then_block, else_block, join) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then: then_block,
                otherwise: else_block,
            },
            span.clone(),
        );
        self.current = then_block;
        self.stmts(then, span.clone());
        self.terminate(TerminatorKind::Goto(join), span.clone());
        self.current = else_block;
        if let Some(otherwise) = otherwise {
            self.stmts(otherwise, span.clone());
        }
        self.terminate(TerminatorKind::Goto(join), span);
        self.current = join;
    }

    /// Ends the locals of every scope from `depth` in, innermost first.
    pub(super) fn end_scopes_from(&mut self, depth: usize, span: Span) {
        let dying: Vec<Local> = (self.scopes[depth..].iter().rev())
            .flat_map(|scope| scope.iter().rev().copied())
            .collect();
        for local in dying {
            self.end_local(local, span.clone());
        }
    }

    /// A named local for `def`, living until the end of the innermost scope.
    pub(super) fn let_local(
        &mut self,
        def: DefId,
        name: &Ident,
        ty: TypeId,
        mutable: bool,
        span: Span,
    ) -> Local {
        let local = self.new_local(ty, Some(name.name.to_string()), mutable, name.span.clone());
        self.bindings.insert(def, local);
        self.declare(local, span);
        local
    }

    /// Lives until the end of the innermost scope.
    pub(super) fn declare(&mut self, local: Local, span: Span) {
        (self.scopes.last_mut())
            .expect("declared inside a scope")
            .push(local);
        self.push(StatementKind::StorageLive(local), span);
    }
}
