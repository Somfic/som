use crate::{Ast, Child, Expr, Id, Stmt};

/// What type checking settles about the code, which decides where control can go.
pub trait Settled {
    fn settled_if(&self, stmt: Id<Stmt>) -> Option<bool>;
    /// The arm-free copy a `match` over a known value was replaced by.
    fn chosen_match(&self, expr: Id<Expr>) -> Option<Id<Expr>>;
}

pub struct Flow<'a, S: Settled + ?Sized> {
    pub ast: &'a Ast,
    pub settled: &'a S,
}

impl<S: Settled + ?Sized> Flow<'_, S> {
    /// Whether control never gets past the end of `stmts`. Any statement that leaves counts:
    /// what's after it is unreachable.
    pub fn leaves(&self, stmts: &[Id<Stmt>]) -> bool {
        stmts.iter().any(|&stmt| self.stmt_leaves(stmt))
    }

    pub fn stmt_leaves(&self, stmt: Id<Stmt>) -> bool {
        match &self.ast.stmts[stmt] {
            Stmt::Return(_) | Stmt::Break(_) | Stmt::Continue(_) => true,
            Stmt::If {
                then, otherwise, ..
            } if let Some(holds) = self.settled.settled_if(stmt) => match holds {
                true => self.leaves(then),
                false => otherwise.as_ref().is_some_and(|o| self.leaves(o)),
            },
            Stmt::If {
                then,
                otherwise: Some(otherwise),
                ..
            } => self.leaves(then) && self.leaves(otherwise),
            Stmt::Loop { body } => !self.breaks(body),
            Stmt::Expr(expr) => self.expr_leaves(*expr),
            _ => false,
        }
    }

    pub fn expr_leaves(&self, expr: Id<Expr>) -> bool {
        if let Some(copy) = self.settled.chosen_match(expr) {
            return self.expr_leaves(copy);
        }
        match &self.ast.exprs[expr] {
            Expr::Unreachable | Expr::Fail(_) | Expr::Raise(_) => true,
            Expr::Block { stmts, value } => {
                self.leaves(stmts) || value.is_some_and(|v| self.expr_leaves(v))
            }
            Expr::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|arm| self.expr_leaves(arm.body))
            }
            Expr::Conditional {
                then, otherwise, ..
            } => self.expr_leaves(*then) && self.expr_leaves(*otherwise),
            _ => false,
        }
    }

    /// Whether a `break` in `stmts` may leave the loop they're in.
    pub fn breaks(&self, stmts: &[Id<Stmt>]) -> bool {
        stmts
            .iter()
            .any(|&stmt| self.child_breaks(Child::Stmt(stmt)))
    }

    fn child_breaks(&self, child: Child) -> bool {
        let mut found = false;
        match child {
            Child::Stmt(stmt) => match &self.ast.stmts[stmt] {
                Stmt::Break(_) => return true,
                other => other.for_each_child(|c| found |= self.child_breaks(c)),
            },
            Child::Expr(expr) => match self.settled.chosen_match(expr) {
                Some(copy) => return self.child_breaks(Child::Expr(copy)),
                None => self.ast.exprs[expr].for_each_child(|c| found |= self.child_breaks(c)),
            },
        }
        found
    }
}
