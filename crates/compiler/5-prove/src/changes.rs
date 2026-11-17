//! What may change a place, so what's known about it is forgotten.

use som_ast::{BinOp, Child, Expr, Flow, Id, Stmt};
use som_typeck::Typed;

use crate::{Place, Prover};

impl Prover<'_> {
    /// `place` may have changed: what's known about it, and inside it, is forgotten. An
    /// element, `xs[i]`, changes on its own: `at_mut` lends it without changing `xs`.
    pub(crate) fn changed(&mut self, place: Id<Expr>) {
        if let Some(changed) = self.place_of(place) {
            self.facts
                .forget(|p| p.within(&changed) || changed.within(p));
        }
    }

    /// The variable, or field of one, expression `expr` is a place in: `xs` for `xs[i]`.
    fn place_of(&self, expr: Id<Expr>) -> Option<Place> {
        match &self.ast.exprs[expr] {
            Expr::Var(_) => Some(Place::var(self.names.expr(expr)?)),
            Expr::Field { object, field } => match self.place_of(*object)? {
                Place::Path { root, mut path } => {
                    path.push(field.name.clone());
                    Some(Place::Path { root, path })
                }
                len => Some(len),
            },
            Expr::Deref(inner) => self.place_of(*inner),
            _ => None,
        }
    }

    /// Before a loop: forgets what its body may change, since a later time round sees it.
    pub(crate) fn forget_changed_in(&mut self, body: &[Id<Stmt>]) {
        if self.facts.is_empty() {
            return;
        }
        let mut changed = Vec::new();
        for &stmt in body {
            self.stmt_exprs(stmt, &mut |e| self.changes_in(e, &mut changed));
        }
        // A counter only ever counted up, `i = i + 1`, is still at least what it was.
        let rising: Vec<Place> = (changed.iter())
            .filter(|place| matches!(place, Place::Path { path, .. } if path.is_empty()))
            .filter(|&place| self.only_rises(body, place))
            .cloned()
            .collect();
        let touches = |place: &Place, p: &Place| p.within(place) || place.within(p);
        for place in &changed {
            if rising.contains(place) {
                let others: Vec<&Place> = changed.iter().filter(|c| *c != place).collect();
                self.facts.forget_rising(place, |p| {
                    touches(place, p) || others.iter().any(|o| touches(o, p))
                });
            } else {
                self.facts.forget(|p| touches(place, p));
            }
        }
    }

    /// Whether every change to variable `place` in `body` adds a number not below 0 to it:
    /// `i = i + 1`, `i += 2`.
    fn only_rises(&self, body: &[Id<Stmt>], place: &Place) -> bool {
        let mut rises = true;
        for &stmt in body {
            self.stmt_exprs_deep(stmt, &mut |expr| match self.ast.exprs[expr] {
                Expr::Assign { target, value } if self.place_of(target).as_ref() == Some(place) => {
                    rises &= self.adds_to(value, place);
                }
                Expr::Assign { .. } => {}
                // Lent as `&mut`, or to a method that may change it: anything could happen.
                _ => {
                    let mut changed = Vec::new();
                    self.changes_here(expr, &mut changed);
                    rises &= !changed.iter().any(|c| c.within(place) || place.within(c));
                }
            });
        }
        rises
    }

    /// Whether `value` is `place` plus a number not below 0.
    fn adds_to(&self, value: Id<Expr>, place: &Place) -> bool {
        let Expr::Binary {
            op: BinOp::Add,
            lhs,
            rhs,
        } = self.ast.exprs[value]
        else {
            return false;
        };
        match (&self.ast.exprs[lhs], &self.ast.exprs[rhs]) {
            (_, &Expr::Int(n)) => n >= 0 && self.place_of(lhs).as_ref() == Some(place),
            (&Expr::Int(n), _) => n >= 0 && self.place_of(rhs).as_ref() == Some(place),
            _ => false,
        }
    }

    /// The places `expr` itself may change, not counting what's inside it: assigned, lent as
    /// `&mut`, or given to `&mut self`.
    fn changes_here(&self, expr: Id<Expr>, changed: &mut Vec<Place>) {
        let target = match &self.ast.exprs[expr] {
            Expr::Assign { target, .. } => Some(*target),
            Expr::Borrow {
                mutable: true,
                expr,
            } => Some(*expr),
            Expr::MethodCall { receiver, .. }
                if self.method(expr).is_some_and(|f| self.takes_mut_self(f)) =>
            {
                Some(*receiver)
            }
            _ => None,
        };
        if let Some(place) = target.and_then(|t| self.place_of(t)) {
            changed.push(place);
        }
    }

    /// The places `expr`, or anything inside it, may change.
    fn changes_in(&self, expr: Id<Expr>, changed: &mut Vec<Place>) {
        self.changes_here(expr, changed);
        self.each_child(expr, &mut |child| self.changes_in(child, changed));
    }

    /// Calls `f` with every expression in `stmt`, however deep.
    fn stmt_exprs_deep(&self, stmt: Id<Stmt>, f: &mut dyn FnMut(Id<Expr>)) {
        fn walk(p: &Prover, expr: Id<Expr>, f: &mut dyn FnMut(Id<Expr>)) {
            f(expr);
            p.each_child(expr, &mut |child| walk(p, child, f));
        }
        self.stmt_exprs(stmt, &mut |e| walk(self, e, f));
    }

    /// Calls `f` with each expression directly inside `expr`, and each statement's.
    pub(crate) fn each_child(&self, expr: Id<Expr>, f: &mut dyn FnMut(Id<Expr>)) {
        if let Some(copy) = self.typed.chosen_match(expr) {
            return f(copy);
        }
        self.ast.exprs[expr].for_each_child(|child| match child {
            Child::Expr(e) => f(e),
            Child::Stmt(s) => self.stmt_exprs(s, f),
        });
    }

    fn stmt_exprs(&self, stmt: Id<Stmt>, f: &mut dyn FnMut(Id<Expr>)) {
        self.ast.stmts[stmt].for_each_child(|child| match child {
            Child::Expr(e) => f(e),
            Child::Stmt(s) => self.stmt_exprs(s, f),
        });
    }

    pub(crate) fn flow(&self) -> Flow<'_, Typed> {
        Flow {
            ast: self.ast,
            settled: self.typed,
        }
    }
}
