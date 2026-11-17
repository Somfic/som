use std::collections::HashSet;

use som_ast::{BinOp, Child, Expr, Id, Stmt};
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId};

/// What a condition says about optional places when it's true, and when it's false.
#[derive(Default)]
pub(super) struct Facts {
    pub(super) when_true: Vec<Narrowing>,
    pub(super) when_false: Vec<Narrowing>,
}

impl Checker<'_> {
    /// Checks a condition: a `bool`, or an optional, which is true when it holds a value
    /// (`if x` is `if x != none`), combined with `&&`, `||` and `!`. Returns what it being
    /// true, or false, says about optional places.
    pub(super) fn check_condition(&mut self, condition: Id<Expr>) -> Facts {
        let bool = self.u.table.bool;
        let facts = match &self.ast.exprs[condition] {
            // The right side only runs when the left is true: `x != none and x > 3`.
            Expr::Binary {
                op: BinOp::And,
                lhs,
                rhs,
            } => {
                let left = self.check_condition(*lhs);
                let (right, left_true) =
                    self.narrowed_during(left.when_true, |c| c.check_condition(*rhs));
                Facts {
                    when_true: left_true.into_iter().chain(right.when_true).collect(),
                    when_false: Vec::new(),
                }
            }
            // The right side only runs when the left is false: `x == none or x > 3`.
            Expr::Binary {
                op: BinOp::Or,
                lhs,
                rhs,
            } => {
                let left = self.check_condition(*lhs);
                let (right, left_false) =
                    self.narrowed_during(left.when_false, |c| c.check_condition(*rhs));
                Facts {
                    when_true: Vec::new(),
                    when_false: left_false.into_iter().chain(right.when_false).collect(),
                }
            }
            Expr::Not(inner) => {
                let inner = self.check_condition(*inner);
                Facts {
                    when_true: inner.when_false,
                    when_false: inner.when_true,
                }
            }
            _ => return self.check_condition_operand(condition),
        };
        self.exprs[condition.id] = Some(bool);
        facts
    }

    /// Checks a condition that isn't `&&`, `||` or `!`.
    fn check_condition_operand(&mut self, condition: Id<Expr>) -> Facts {
        let mut facts = Facts::default();
        match self.condition_narrowing(condition) {
            Some((narrowing, true)) => facts.when_true.push(narrowing),
            Some((narrowing, false)) => facts.when_false.push(narrowing),
            None => {}
        }
        facts
    }

    /// Checks a condition that isn't `&&`, `||` or `!`, returning the optional place it tells
    /// something about, and whether that holds a value when the condition is true.
    fn condition_narrowing(&mut self, condition: Id<Expr>) -> Option<(Narrowing, bool)> {
        let bool = self.u.table.bool;
        let ty = self.infer_optional(condition);
        let shallow = self.u.shallow(ty);
        if let Ty::Optional(inner) = *self.u.get(shallow) {
            if matches!(self.u.shallow_ty(inner), Ty::Bool) {
                self.errors.push(TypeError::OptionalBoolCondition {
                    span: self.span(condition),
                });
                return None;
            }
            return Some((self.narrowing(condition, inner)?, true));
        }
        if !self.coerce(condition, bool, ty) {
            self.mismatch(bool, ty, self.span(condition), None);
            return None;
        }
        // `x != none`, `x == none`, `x == 5` and `x != 5`.
        let Expr::Binary { op, lhs, rhs } = &self.ast.exprs[condition] else {
            return None;
        };
        let equals = match op {
            BinOp::Equals => true,
            BinOp::NotEquals => false,
            _ => return None,
        };
        let against_none = matches!(self.ast.exprs[*lhs], Expr::None)
            || matches!(self.ast.exprs[*rhs], Expr::None);
        // Equal to `none` means it holds nothing; equal to a value means it holds one.
        let holds_value = equals != against_none;
        let is_optional = |c: &Self, e: Id<Expr>| {
            c.exprs[e.id].is_some_and(|ty| matches!(c.u.shallow_ty(ty), Ty::Optional(_)))
        };
        let optional = match (&self.ast.exprs[*lhs], &self.ast.exprs[*rhs]) {
            (_, Expr::None) => *lhs,
            (Expr::None, _) => *rhs,
            // `x == 5`: only equal if `x` holds a value. Not for two optionals: both could be
            // `none`.
            _ => match (is_optional(self, *lhs), is_optional(self, *rhs)) {
                (true, false) => *lhs,
                (false, true) => *rhs,
                _ => return None,
            },
        };
        let ty = self.exprs[optional.id]?;
        let Ty::Optional(inner) = *self.u.shallow_ty(ty) else {
            return None;
        };
        Some((self.narrowing(optional, inner)?, holds_value))
    }

    /// A narrowing of `expr` to the `inner` type it holds, if it's a place that can be narrowed.
    fn narrowing(&mut self, expr: Id<Expr>, inner: TypeId) -> Option<Narrowing> {
        let key = self.key(expr)?;
        self.next_narrowing += 1;
        Some(Narrowing {
            id: self.next_narrowing,
            key,
            inner,
        })
    }

    /// After an optional place of type `ty` is assigned `value`: if that was a plain value,
    /// wrapped, the place holds it until the end of the block.
    pub(super) fn narrow_if_assigned_a_value(
        &mut self,
        key: Option<Key>,
        ty: TypeId,
        value: Id<Expr>,
    ) {
        let Some(key) = key else { return };
        if !self.records.wrapped.contains(&value) {
            return;
        }
        let Ty::Optional(inner) = *self.u.shallow_ty(ty) else {
            return;
        };
        self.next_narrowing += 1;
        self.narrowed.push(Narrowing {
            id: self.next_narrowing,
            key,
            inner,
        });
    }

    /// Runs `f` with `narrowings` in effect, returning its result and the narrowings it didn't
    /// end.
    pub(super) fn narrowed_during<T>(
        &mut self,
        narrowings: Vec<Narrowing>,
        f: impl FnOnce(&mut Self) -> T,
    ) -> (T, Vec<Narrowing>) {
        let ids: HashSet<usize> = narrowings.iter().map(|n| n.id).collect();
        self.narrowed.extend(narrowings);
        let result = f(self);
        // They may have ended early; others ended inside stay ended.
        let (still, others): (Vec<Narrowing>, Vec<Narrowing>) = std::mem::take(&mut self.narrowed)
            .into_iter()
            .partition(|n| ids.contains(&n.id));
        self.narrowed = others;
        (result, still)
    }
}

// --- Narrowing ---
//
// Inside `if x { … }` (or `if x != none`), where `x` is an optional variable or field, `x`
// means the value it holds. Anything that could make `x` `none` again ends that: assigning
// it or what contains it, `&mut` of what contains it, or passing on a `&mut` it's behind.
// A loop is checked for those first, since a later iteration sees changes further down.

/// An optional place known to hold a value.
#[derive(Clone)]
pub(super) struct Narrowing {
    pub(super) id: usize,
    pub(super) key: Key,
    inner: TypeId,
}

/// A place as written: a variable, then fields and dereferences.
#[derive(Clone, PartialEq)]
pub(super) struct Key {
    pub(super) root: DefId,
    pub(super) path: Vec<Step>,
}

#[derive(Clone, PartialEq)]
pub(super) enum Step {
    Field(Box<str>),
    Deref,
}

impl Key {
    /// The fields, without the dereferences between them: `p.x` and `(*p).x` are the same
    /// memory, since a field is reached through a reference without writing the `*`.
    fn fields(&self) -> impl Iterator<Item = &Step> {
        self.path.iter().filter(|s| matches!(s, Step::Field(_)))
    }

    /// How many dereferences end the path: `*r` is what `r` points to, not `r` itself.
    fn trailing_derefs(&self) -> usize {
        self.path
            .iter()
            .rev()
            .take_while(|s| **s == Step::Deref)
            .count()
    }

    /// Whether this place is `other`, or inside it.
    pub(super) fn within(&self, other: &Key) -> bool {
        let mut mine = self.fields();
        let prefix =
            self.root == other.root && other.fields().all(|step| mine.next() == Some(step));
        // Past `other`'s fields, it's inside; at the same fields, it's `other` itself or what
        // `other` points to, not a reference `other` is behind.
        prefix && (mine.next().is_some() || other.trailing_derefs() <= self.trailing_derefs())
    }

    pub(super) fn same(&self, other: &Key) -> bool {
        self.within(other) && other.within(self)
    }
}

impl Checker<'_> {
    /// The place `expr` denotes, if it's a variable, field or dereference of one.
    pub(super) fn key(&self, expr: Id<Expr>) -> Option<Key> {
        match &self.ast.exprs[expr] {
            Expr::Var(_) => {
                let def = self.names.expr(expr)?;
                matches!(
                    self.names.def(def).kind,
                    DefKind::Local(_)
                        | DefKind::Param { .. }
                        | DefKind::Out(_)
                        | DefKind::Binding(_)
                )
                .then(|| Key {
                    root: def,
                    path: Vec::new(),
                })
            }
            Expr::Field { .. } if self.is_prop(expr) => None,
            Expr::Field { object, field } => {
                let mut key = self.key(*object)?;
                key.path.push(Step::Field(field.name.clone()));
                Some(key)
            }
            Expr::Deref(inner) => {
                let mut key = self.key(*inner)?;
                key.path.push(Step::Deref);
                Some(key)
            }
            _ => None,
        }
    }

    /// The type `expr` holds, if it's a narrowed place. Only the same spelling matches, so
    /// `(*p).x` isn't narrowed by `if p.x`, which is safe, just less convenient.
    pub(super) fn narrowed_type(&self, expr: Id<Expr>) -> Option<TypeId> {
        if self.narrowed.is_empty() {
            return None;
        }
        let key = self.key(expr)?;
        let found = self.narrowed.iter().rev().find(|n| n.key == key)?;
        Some(found.inner)
    }

    /// Ends the narrowings of the places `changed` says could change.
    pub(super) fn forget(&mut self, changed: impl Fn(&Key) -> bool) {
        self.narrowed.retain(|n| !changed(&n.key));
    }

    pub(super) fn is_mut_ref(&self, ty: TypeId) -> bool {
        matches!(self.u.shallow_ty(ty), Ty::Ref { mutable: true, .. })
    }

    /// Ends the narrowings of the places `stmts` could change.
    pub(super) fn forget_changed_in(&mut self, stmts: &[Id<Stmt>]) {
        if self.narrowed.is_empty() {
            return;
        }
        for &stmt in stmts {
            let mut children = Vec::new();
            self.ast.stmts[stmt].for_each_child(|c| children.push(c));
            self.forget_changed_by_children(children);
        }
    }

    fn forget_changed_by_children(&mut self, children: Vec<Child>) {
        for child in children {
            match child {
                Child::Expr(e) => self.forget_changed_by(e, false),
                Child::Stmt(s) => self.forget_changed_in(&[s]),
            }
        }
    }

    /// Ends the narrowings of the places `expr` could change. `in_place`: it's the object of
    /// a field access or dereference.
    fn forget_changed_by(&mut self, expr: Id<Expr>, in_place: bool) {
        if let Some(&copy) = self.records.chosen_matches.get(&expr) {
            return self.forget_changed_by(copy, false);
        }
        match &self.ast.exprs[expr] {
            Expr::Var(_) => {
                if !in_place
                    && let Some(def) = self.names.expr(expr)
                    && let Some(&ty) = self.locals.get(&def)
                    && self.is_mut_ref(ty)
                {
                    self.forget(|key| key.root == def && !key.path.is_empty());
                }
            }
            Expr::Field { object, .. } | Expr::Deref(object) => {
                self.forget_changed_by(*object, true)
            }
            Expr::Index { object, index } => {
                self.forget_changed_by(*object, true);
                self.forget_changed_by(*index, false);
            }
            Expr::Borrow { mutable, expr } => {
                if *mutable && let Some(borrowed) = self.key(*expr) {
                    self.forget(|key| key.within(&borrowed) && !key.same(&borrowed));
                }
                self.forget_changed_by(*expr, false);
            }
            Expr::Assign { target, value } => {
                if let Some(assigned) = self.key(*target) {
                    self.forget(|key| key.within(&assigned));
                }
                self.forget_changed_by(*target, true);
                self.forget_changed_by(*value, false);
            }
            other => {
                let mut children = Vec::new();
                other.for_each_child(|c| children.push(c));
                self.forget_changed_by_children(children);
            }
        }
    }
}
