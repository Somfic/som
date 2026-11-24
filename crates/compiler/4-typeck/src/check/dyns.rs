//! `dyn Trait`: a value of any type implementing the trait, which one known only as the
//! program runs. Its methods are called through the functions it holds for them, so each has
//! to be callable without knowing the type: taking `&self` or `&mut self`, and no other `Self`.

use som_ast::{Id, Path, Span, TypeExpr};
use som_base::message::{Message, code};
use som_base::msg;
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId};

impl Checker<'_> {
    /// `dyn Display`, `dyn Index<usize>`. Whether the trait can be one is checked once every
    /// signature is known: `check_dyns`.
    pub(super) fn lower_dyn(&mut self, id: Id<TypeExpr>, path: &Path, args: &[Id<TypeExpr>]) -> Ty {
        let given: Vec<TypeId> = args.iter().map(|&a| self.lower(a)).collect();
        let Some(def) = self.names.ty(id) else {
            return Ty::Error; // already reported by the resolver
        };
        let DefKind::Trait(t) = self.names.def(def).kind else {
            return Ty::Error;
        };
        let error = self.u.table.error;
        let args = self.trait_args(def, t, error, given, path.span());
        self.dyns.push((def, self.ast.types.span(id).clone()));
        Ty::Dyn(def, args)
    }

    /// Reports each `dyn` of a trait with a method that can't be called through one.
    pub(super) fn check_dyns(&mut self) {
        let mut seen: Vec<(DefId, Span)> = Vec::new();
        for (trait_, span) in std::mem::take(&mut self.dyns) {
            if seen.contains(&(trait_, span.clone())) {
                continue;
            }
            seen.push((trait_, span.clone()));
            if let Some((why, member)) = self.not_dyn(trait_) {
                self.errors.push(TypeError::NotDyn {
                    trait_: self.names.def(trait_).name.to_string(),
                    why,
                    span,
                    member,
                });
            }
        }
    }

    /// Why `trait_` can't be a `dyn`, and the member that's why. `None` if it can.
    pub(crate) fn not_dyn(&self, trait_: DefId) -> Option<(Message, Option<Span>)> {
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            return None;
        };
        if let Some(&assoc) = self.names.trait_types(t).first() {
            let name = self.names.def(assoc).name.to_string();
            return Some((msg!["it has a type, ", code(name)], Some(self.names.def(assoc).span.clone())));
        }
        if let Some(&parent) = self.names.supers(t).iter().find(|&&s| self.has_methods(s)) {
            let name = self.names.def(parent).name.to_string();
            return Some((msg!["it builds on ", code(name)], None));
        }
        for &m in &self.ast.traits[t].methods {
            let func = &self.ast.funcs[m];
            // One with a body is called with `Self` the `dyn`, through the others.
            if func.body.is_some() {
                continue;
            }
            // Each method has a `Self` of its own, its first generic parameter.
            let Some(&this) = self.names.type_params(m).first() else {
                continue;
            };
            let name = func.name.name.to_string();
            let at = Some(func.name.span.clone());
            let sig = &self.sigs[m.id];
            let receiver = sig.params.first().map(|&p| self.u.get(p).clone());
            let lent = matches!(receiver, Some(Ty::Ref { to, .. }) if *self.u.get(to) == Ty::Param(this));
            if !lent {
                let why = match receiver {
                    Some(Ty::Param(p)) if p == this => msg![code(name), " takes ", code("self"), " by value"],
                    _ => msg![code(name), " has no ", code("self")],
                };
                return Some((why, at));
            }
            // `Self` and the trait's, then its own.
            if self.names.type_params(m).len() > self.names.trait_params(t).len() {
                return Some((msg![code(name), " has type parameters of its own"], at));
            }
            let mentions = |ty: TypeId| self.mentions(ty, this);
            if sig.params[1..].iter().any(|&p| mentions(p)) {
                return Some((msg![code(name), " takes a ", code("Self")], at));
            }
            if mentions(sig.ret) {
                return Some((msg![code(name), " gives a ", code("Self")], at));
            }
        }
        None
    }

    /// Whether a `found` can be made a `dyn trait_<args>`: its type is known, and implements
    /// the trait.
    pub(super) fn makes_dyn(&mut self, found: TypeId, trait_: DefId, args: &[TypeId]) -> bool {
        use crate::unify::VarKind;
        let found = self.u.shallow(found);
        // A number or text nothing's decided the type of yet is its usual type.
        let usual = match self.u.var_kind(found) {
            Some(VarKind::Int) => Some(self.u.table.i32),
            Some(VarKind::Float) => Some(self.u.table.f64),
            Some(VarKind::Text) => Some(self.u.table.text),
            _ => None,
        };
        let found = match usual {
            Some(usual) if self.implements_given(usual, trait_, args) => {
                let _ = self.u.unify(found, usual);
                usual
            }
            _ => found,
        };
        !matches!(self.u.get(found), Ty::Error | Ty::Dyn(..) | Ty::Var(_))
            && self.implements_given(found, trait_, args)
    }

    fn has_methods(&self, trait_: DefId) -> bool {
        match self.names.def(trait_).kind {
            DefKind::Trait(t) => !self.ast.traits[t].methods.is_empty(),
            _ => false,
        }
    }

    /// Whether `ty` is or holds type parameter `param`.
    fn mentions(&self, ty: TypeId, param: DefId) -> bool {
        let ty = self.u.get(ty);
        *ty == Ty::Param(param) || ty.children().into_iter().any(|c| self.mentions(c, param))
    }
}
