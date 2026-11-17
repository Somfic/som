use som_ast::{Id, Span, Stmt};
use som_mir::*;
use som_resolve::DefKind;

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
        // The statement's temporaries are done with: dropped, unless they were moved.
        let temps: Vec<Local> = (first_new..self.locals.slots())
            .map(Id::new)
            .filter(|&l| self.locals[l].name.is_none())
            .collect();
        for temp in temps {
            self.drop_local(temp, span.clone());
        }
    }

    pub(super) fn stmt_inner(&mut self, id: Id<Stmt>, span: Span) {
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
            } => {
                let value = self.rvalue(*value);
                let def = self.l.names.local(id).expect("every let is resolved");
                let ty = self.typed.local(def);
                let ty = self.ty(ty);
                let local =
                    self.new_local(ty, Some(name.name.to_string()), *mutable, name.span.clone());
                self.bindings.insert(def, local);
                self.scopes
                    .last_mut()
                    .expect("lets are inside a scope")
                    .push(local);
                self.push(StatementKind::StorageLive(local), span.clone());
                self.assign(Place::local(local), value, span);
            }
            // `@for f in T` over a generic type: the copy for this instance's `T`.
            Stmt::Expr(_) if let Some(arm) = self.typed.chosen_arm(id) => {
                self.operand(arm);
            }
            Stmt::Expr(expr) => {
                // The result is unused: a temporary holding it dies with the statement.
                if let Operand::Copy(place) | Operand::Move(place) = self.operand(*expr)
                    && place.projection.is_empty()
                    && place.local != RETURN
                    && self.locals[place.local].name.is_none()
                {
                    self.end_local(place.local, span);
                }
            }
            Stmt::Loop { body } => {
                let (start, exit) = (self.new_block(), self.new_block());
                self.terminate(TerminatorKind::Goto(start), span.clone());
                self.current = start;
                self.loop_body(body, exit, start, span.clone());
                self.terminate(TerminatorKind::Goto(start), span);
                // Only a `break` gets here.
                self.current = exit;
            }
            Stmt::While { condition, body } => {
                let (header, body_block, exit) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.terminate(TerminatorKind::Goto(header), span.clone());
                self.current = header;
                let condition = self.operand(*condition);
                self.terminate(
                    TerminatorKind::Branch {
                        condition,
                        then: body_block,
                        otherwise: exit,
                    },
                    span.clone(),
                );
                self.current = body_block;
                self.loop_body(body, exit, header, span.clone());
                self.terminate(TerminatorKind::Goto(header), span);
                self.current = exit;
            }
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
            Stmt::Return(value) => {
                let value = match value {
                    Some(value) => self.operand(*value),
                    None => {
                        let unit = self.l.mir.types.unit;
                        self.constant(ConstKind::Unit, unit)
                    }
                };
                self.assign(Place::local(RETURN), Rvalue::Use(value), span.clone());
                // Every local still alive ends here: each enclosing scope's, then the parameters.
                let dying: Vec<Local> = (self.scopes.iter().rev())
                    .flat_map(|scope| scope.iter().rev().copied())
                    .collect();
                for local in dying {
                    self.end_local(local, span.clone());
                }
                for arg in (1..=self.param_count()).rev() {
                    self.end_local(Id::new(arg), span.clone());
                }
                self.terminate(TerminatorKind::Return, span);
                // Whatever follows can't be reached.
                self.current = self.new_block();
            }
            Stmt::Break(named) | Stmt::Continue(named) => {
                // The innermost loop, or the `for` loop whose variable is named.
                let targets = match named {
                    None => *self.loops.last().expect("checked: break is in a loop"),
                    Some(_) => {
                        let def = self
                            .l
                            .names
                            .loop_target(id)
                            .expect("resolved: it names a loop");
                        let DefKind::Local(for_stmt) = self.l.names.def(def).kind else {
                            unreachable!("resolved: loop variables are locals")
                        };
                        *(self.loops.iter().rev())
                            .find(|l| l.stmt == Some(for_stmt))
                            .expect("resolved: the loop it names is around it")
                    }
                };
                // The loop's own locals end here, as they would at the end of the body.
                let dying: Vec<Local> = (self.scopes[targets.depth..].iter().rev())
                    .flat_map(|scope| scope.iter().rev().copied())
                    .collect();
                for local in dying {
                    self.end_local(local, span.clone());
                }
                let target = match ast.stmts[id] {
                    Stmt::Break(_) => targets.exit,
                    _ => targets.next,
                };
                self.terminate(TerminatorKind::Goto(target), span);
                // Whatever follows in the body can't be reached.
                self.current = self.new_block();
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
            } => {
                let condition = self.condition(*condition);
                let (then_block, else_block, join) =
                    (self.new_block(), self.new_block(), self.new_block());
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
        }
    }
}
