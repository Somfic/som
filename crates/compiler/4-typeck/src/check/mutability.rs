use som_ast::{Expr, Id, Pattern, Stmt};
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::error::TypeError;
use crate::ty::Ty;

/// Why a place can't be changed.
pub(super) enum Immutable {
    /// A variable declared without `mut`.
    Variable(DefId),
    /// A constant: the same everywhere, never changed.
    Const(DefId),
    BehindSharedReference,
    /// What a `#T` shares: every copy sees it, so nothing changes it.
    Shared,
}

impl Checker<'_> {
    /// Reports it if the place `expr` can't be changed: assigned to, or with `borrow`, borrowed
    /// as `&mut`.
    pub(super) fn check_mutable(&mut self, expr: Id<Expr>, borrow: bool) {
        // `xs[i].x = 1`: through `at_mut`, which checks `xs` can change.
        let mut place = expr;
        loop {
            match self.ast.exprs[place] {
                Expr::Field { object, .. }
                    if !self.is_prop(place) && self.reference(object).is_none() =>
                {
                    place = object
                }
                Expr::Index { .. } if self.records.methods.contains_key(&place) => {
                    self.index_mut(place);
                    return;
                }
                _ => break,
            }
        }
        let error = match self.immutable(expr) {
            Some(Immutable::Variable(def)) => {
                let def = self.names.def(def);
                TypeError::NotMutable {
                    name: def.name.to_string(),
                    span: self.span(expr),
                    declared_at: def.span.clone(),
                    borrow,
                }
            }
            Some(Immutable::Const(def)) => {
                let def = self.names.def(def);
                TypeError::ConstChanged {
                    name: def.name.to_string(),
                    span: self.span(expr),
                    defined_at: def.span.clone(),
                    borrow,
                }
            }
            Some(Immutable::BehindSharedReference) => TypeError::BehindSharedReference {
                span: self.span(expr),
                borrow,
            },
            Some(Immutable::Shared) => TypeError::SharedChanged {
                span: self.span(expr),
                borrow,
            },
            None => return,
        };
        self.errors.push(error);
    }

    /// Why the place `expr` can't be changed, if it can't. `out` parameters and variables can
    /// be: being filled in is what they're for. Anything that isn't a place, like a call's
    /// result, is a fresh value.
    fn immutable(&self, expr: Id<Expr>) -> Option<Immutable> {
        match &self.ast.exprs[expr] {
            Expr::Var(_) => {
                let def = self.names.expr(expr)?;
                let mutable = match self.names.def(def).kind {
                    DefKind::Const(_) => return Some(Immutable::Const(def)),
                    DefKind::Local(stmt) => matches!(
                        self.ast.stmts[stmt],
                        Stmt::Let { mutable: true, .. } | Stmt::For { mutable: true, .. }
                    ),
                    DefKind::Param { func, index } => {
                        let param = &self.ast.funcs[func].params[index];
                        param.mutable || param.out
                    }
                    DefKind::Binding(pattern) => matches!(
                        self.ast.patterns[pattern],
                        Pattern::Binding { mutable: true, .. }
                    ),
                    _ => true,
                };
                (!mutable).then_some(Immutable::Variable(def))
            }
            // Through a reference, what counts is the reference: `&mut` allows it, even in a
            // variable that isn't `mut`.
            Expr::Field { .. } if self.is_prop(expr) => None,
            // What a `#T` shares is lent by its `get`: shared, never changed.
            Expr::Field { object, .. } | Expr::Index { object, .. }
                if self.records.shared.contains(object) =>
            {
                Some(Immutable::Shared)
            }
            Expr::Field { object, .. } | Expr::Index { object, .. } => {
                match self.reference(*object) {
                    Some(true) => None,
                    Some(false) => Some(Immutable::BehindSharedReference),
                    None => self.immutable(*object),
                }
            }
            Expr::Deref(inner) => match self.reference(*inner) {
                Some(false) => Some(Immutable::BehindSharedReference),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether `expr` is a `&mut` reference, a `&` one, or not a reference.
    fn reference(&self, expr: Id<Expr>) -> Option<bool> {
        let ty = self.exprs[expr.id]?;
        match self.u.shallow_ty(ty) {
            Ty::Ref { mutable, .. } => Some(*mutable),
            _ => None,
        }
    }

    /// Whether `expr` denotes a memory location that can be assigned to.
    pub(super) fn is_place(&self, expr: Id<Expr>) -> bool {
        match &self.ast.exprs[expr] {
            Expr::Var(_) => self.names.expr(expr).is_none_or(|def| {
                matches!(
                    self.names.def(def).kind,
                    DefKind::Local(_)
                        | DefKind::Out(_)
                        | DefKind::Param { .. }
                        | DefKind::Binding(_)
                        // One that can't change, reported as that.
                        | DefKind::Const(_)
                )
            }),
            Expr::Field { object, .. } => !self.is_prop(expr) && self.is_place(*object),
            Expr::Index { object, .. } => self.is_place(*object),
            Expr::Deref(_) => true,
            _ => false,
        }
    }
}
