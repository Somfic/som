//! Moving out of a reference in generic code: `*item` of a `&T`, or `p.value` of a
//! `&Pair<T>`, where `T` may be a type that moves. The borrow checker sees each use of a
//! function with its type arguments filled in, so it would only find this once someone uses it
//! with such a type; here it's an error where it's written, whether it's used or not.

use som_ast::{Expr, Id, Stmt};
use som_resolve::LangTrait;

use super::Checker;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId};

impl Checker<'_> {
    /// Reports each value in `body` moved out of a reference whose type may move because of a
    /// type parameter.
    pub(super) fn check_moves_out(&mut self, body: Id<Expr>) {
        self.moves_in(body, true);
    }

    /// `by_value`: what `id` gives is taken, rather than borrowed or assigned to.
    fn moves_in(&mut self, id: Id<Expr>, by_value: bool) {
        match &self.ast.exprs[id] {
            Expr::Hole
            | Expr::Unreachable
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None
            | Expr::Var(_)
            | Expr::Out(_) => {}
            // What's written into text is only read.
            Expr::Interpolated(parts) => {
                for &part in parts {
                    if let som_ast::Part::Value(value) = part {
                        self.moves_in(value, false);
                    }
                }
            }
            Expr::Deref(inner) => {
                if by_value {
                    self.check_move_out(id);
                }
                self.moves_in(*inner, false);
            }
            Expr::Field { object, .. } => {
                // A property is a call, giving a value of its own; its object is borrowed.
                let place =
                    self.methods.get(&id).is_none() && self.array_methods.get(&id).is_none();
                if by_value && place && self.behind_reference(id) {
                    self.check_move_out(id);
                }
                self.moves_in(*object, false);
            }
            Expr::Index { object, index } => {
                if by_value && self.behind_reference(id) {
                    self.check_move_out(id);
                }
                self.moves_in(*object, false);
                self.moves_in(*index, true);
            }
            Expr::Borrow { expr, .. } => self.moves_in(*expr, false),
            Expr::Assign { target, value } => {
                self.moves_in(*target, false);
                self.moves_in(*value, true);
            }
            Expr::Array(items) => {
                for &item in items {
                    self.moves_in(item, true);
                }
            }
            Expr::Cast { value, .. } | Expr::ArrayRepeat { value, .. } => {
                self.moves_in(*value, true)
            }
            Expr::Not(inner) | Expr::Neg(inner) | Expr::BitNot(inner) => {
                self.moves_in(*inner, true)
            }
            Expr::Range { start, end } => {
                for &bound in start.iter().chain(end) {
                    self.moves_in(bound, true);
                }
            }
            // A trait's operator borrows both sides: `a == b` is `eq(&a, &b)`.
            Expr::Binary { lhs, rhs, .. } => {
                let by_value = !self.trait_ops.contains_key(&id);
                self.moves_in(*lhs, by_value);
                self.moves_in(*rhs, by_value);
            }
            Expr::Call { args, .. } => {
                for &arg in args {
                    self.moves_in(arg, true);
                }
            }
            // The receiver is taken only by a method whose `self` isn't a reference.
            Expr::MethodCall { receiver, args, .. } => {
                let takes_self = (self.methods.get(&id).copied())
                    .and_then(|def| match self.names.def(def).kind {
                        som_resolve::DefKind::Func(func) => {
                            self.sigs[func.id].params.first().copied()
                        }
                        _ => None,
                    })
                    .is_some_and(|self_ty| {
                        !matches!(self.u.get(self.u.shallow(self_ty)), Ty::Ref { .. })
                    });
                self.moves_in(*receiver, takes_self);
                for &arg in args {
                    self.moves_in(arg, true);
                }
            }
            Expr::Construct { fields, .. } => {
                for &(_, value) in fields {
                    self.moves_in(value, true);
                }
            }
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                self.moves_in(*condition, true);
                self.moves_in(*then, by_value);
                self.moves_in(*otherwise, by_value);
            }
            Expr::Fallback { optional, fallback } => {
                self.moves_in(*optional, by_value);
                self.moves_in(*fallback, by_value);
            }
            // The arms' patterns decide what's taken; the borrow checker sees to that.
            Expr::Match { scrutinee, arms } => {
                self.moves_in(*scrutinee, false);
                for arm in arms {
                    self.moves_in(arm.body, by_value);
                }
            }
            Expr::Block { stmts, value } => {
                for &stmt in stmts {
                    self.moves_in_stmt(stmt);
                }
                if let Some(value) = value {
                    self.moves_in(*value, by_value);
                }
            }
        }
    }

    fn moves_in_stmt(&mut self, id: Id<Stmt>) {
        match &self.ast.stmts[id] {
            Stmt::Use(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(None) => {}
            Stmt::Let { value, .. } | Stmt::Expr(value) | Stmt::Return(Some(value)) => {
                self.moves_in(*value, true)
            }
            Stmt::Loop { body } => body.iter().for_each(|&s| self.moves_in_stmt(s)),
            Stmt::While { condition, body } => {
                self.moves_in(*condition, true);
                body.iter().for_each(|&s| self.moves_in_stmt(s));
            }
            Stmt::For { iterable, body, .. } => {
                self.moves_in(*iterable, true);
                body.iter().for_each(|&s| self.moves_in_stmt(s));
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                self.moves_in(*condition, true);
                then.iter().for_each(|&s| self.moves_in_stmt(s));
                otherwise
                    .iter()
                    .flatten()
                    .for_each(|&s| self.moves_in_stmt(s));
            }
        }
    }

    /// Whether place `id` is reached through a reference: `*r`, `r.x` of a `&Point`, `s[i]`
    /// of a slice, or a place inside one of those.
    fn behind_reference(&self, id: Id<Expr>) -> bool {
        match &self.ast.exprs[id] {
            Expr::Deref(_) => true,
            Expr::Field { object, .. } | Expr::Index { object, .. } => {
                self.is_reference(*object) || self.behind_reference(*object)
            }
            _ => false,
        }
    }

    fn is_reference(&self, id: Id<Expr>) -> bool {
        self.exprs[id.id].is_some_and(|ty| matches!(self.u.get(self.u.shallow(ty)), Ty::Ref { .. }))
    }

    /// Reports taking place `id` out of the reference it's behind, if its type may move
    /// because of a type parameter. Types without one the borrow checker sees as they are.
    fn check_move_out(&mut self, id: Id<Expr>) {
        let Some(ty) = self.exprs[id.id] else {
            return;
        };
        if self.is_copy(ty) || !self.has_type_param(ty) {
            return;
        }
        let span = self.ast.exprs.span(id).clone();
        let fix = self.operator_fix(ty, LangTrait::Copy);
        self.errors.push(TypeError::MoveOutOfReference {
            place: span.get_text().to_string(),
            ty: self.describe(ty),
            span,
            fix,
        });
    }

    fn has_type_param(&self, ty: TypeId) -> bool {
        let ty = self.u.shallow(ty);
        match self.u.get(ty) {
            Ty::Param(_) => true,
            t => t.children().into_iter().any(|c| self.has_type_param(c)),
        }
    }
}
