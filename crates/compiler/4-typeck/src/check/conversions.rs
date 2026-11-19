use som_ast::{Expr, Id};
use som_lang::LangTrait;
use som_resolve::DefId;

use super::Checker;
use crate::impls::{self};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

impl Checker<'_> {
    /// Whether `expr`, of type `found`, becomes the struct `expected` through its `From`
    /// implementation: `let list: List<i32> = [1, 2, 3]` is `List::from([1, 2, 3])`. Records it
    /// if so. Only one step, only into a struct that's known, and only when exactly one
    /// implementation could take `found`; the struct's type arguments can follow from it.
    pub(super) fn from_conversion(
        &mut self,
        expr: Id<Expr>,
        expected: TypeId,
        found: TypeId,
    ) -> bool {
        let Some((from, source, fitting)) = self.from_impls(expected, found) else {
            return false;
        };
        let [(target, arg, ref bounded)] = fitting[..] else {
            return false;
        };
        for &(param, fresh) in bounded {
            for &bound in self.names.bounds(param) {
                self.deferred
                    .obligations
                    .push((fresh, bound, Vec::new(), self.span(expr)));
            }
        }
        let (expected, shallow) = (self.u.shallow(expected), self.u.shallow(found));
        if self.u.unify(target, expected).is_err() || self.u.unify(arg, source).is_err() {
            return false;
        }
        if source != shallow {
            let _ = self.u.unify(shallow, source);
        }
        let Some(func) =
            self.trait_method(from, LangTrait::From.method().expect("From has a method"))
        else {
            return false;
        };
        self.conversions
            .insert(expr, (self.names.func(func), vec![expected, source]));
        true
    }

    /// Whether `param` takes `found` with the number inside it, if one's not decided yet,
    /// as the `i32` or `f64` it would be anyway: `&i32` for a `&` of an undecided integer.
    pub(super) fn fits_as_default(&self, param: TypeId, found: TypeId) -> bool {
        let (mut param, mut found) = (self.u.shallow(param), self.u.shallow(found));
        loop {
            match (self.u.get(param), self.u.get(found)) {
                (&Ty::Ref { to: p, .. }, &Ty::Ref { to: f, .. })
                | (&Ty::Optional(p), &Ty::Optional(f)) => {
                    (param, found) = (self.u.shallow(p), self.u.shallow(f));
                }
                _ => break,
            }
        }
        match self.u.var_kind(found) {
            Some(VarKind::Int) => self.u.could_fit(param, self.u.table.i32),
            Some(VarKind::Float) => self.u.could_fit(param, self.u.table.f64),
            _ => true,
        }
    }

    /// Whether a `found` with something in it not known yet, `&_?`, fits more than one of
    /// the `From` implementations making the struct `expected`: which one depends on what
    /// isn't known, and that it isn't is the error, reported where it's made.
    pub(super) fn conversion_undecided(&mut self, expected: TypeId, found: TypeId) -> bool {
        let unknown = |c: &Self, ty: TypeId| {
            let mut stack = vec![ty];
            while let Some(ty) = stack.pop() {
                let ty = c.u.shallow(ty);
                if c.u.var_kind(ty) == Some(VarKind::Any) {
                    return true;
                }
                stack.extend(c.u.get(ty).children());
            }
            false
        };
        unknown(self, found)
            && self
                .from_impls(expected, found)
                .is_some_and(|(.., fitting)| fitting.len() > 1)
    }

    /// Like `from_conversion`, without changing anything: for choosing between functions with
    /// the same name.
    pub(super) fn could_convert(&mut self, expected: TypeId, found: TypeId) -> bool {
        self.from_impls(expected, found)
            .is_some_and(|(.., fitting)| fitting.len() == 1)
    }

    /// The `From` implementations that could make `found` into struct `expected`, each with
    /// its parameters fresh; also the trait, and what `found` counts as (a collection literal
    /// as an array). `None` if `expected` isn't a struct or `found` isn't known well enough.
    fn from_impls(
        &mut self,
        expected: TypeId,
        found: TypeId,
    ) -> Option<(DefId, TypeId, Vec<(TypeId, TypeId, Vec<(DefId, TypeId)>)>)> {
        let from = self.names.lang(LangTrait::From)?;
        let expected = self.u.shallow(expected);
        if matches!(self.u.get(expected), Ty::Var(_) | Ty::Error) {
            return None;
        }
        let shallow = self.u.shallow(found);
        // A collection literal is an array, unless something says otherwise.
        let source = match (self.u.get(shallow), self.u.var_kind(shallow)) {
            (Ty::Error, _) => return None,
            (_, Some(VarKind::Collection { element, len })) => {
                self.u.intern(Ty::Array { element, len })
            }
            (_, Some(VarKind::Any)) => return None,
            _ => shallow,
        };
        let mut fitting = Vec::new();
        for index in 0..self.impls.len() {
            let info = &self.impls[index];
            if info.trait_ != Some(from) {
                continue;
            }
            let (Some(target), Some(&arg)) = (info.target, info.trait_args.first()) else {
                continue;
            };
            let subst = self.fresh_subst(&info.params.clone());
            let target = impls::substitute(&mut self.u.table, target, &subst);
            let arg = impls::substitute(&mut self.u.table, arg, &subst);
            if self.u.could_fit(target, expected) && self.u.could_fit(arg, source) {
                fitting.push((target, arg, subst));
            }
        }
        // A value fits where its optional is expected too: `5` fits `From<i32?>`. The one
        // taking it as it is goes first.
        if fitting.len() > 1 && !matches!(self.u.get(shallow), Ty::Optional(_)) {
            let exact: Vec<_> = (fitting.iter())
                .filter(|&&(_, arg, _)| !matches!(self.u.shallow_ty(arg), Ty::Optional(_)))
                .cloned()
                .collect();
            if !exact.is_empty() {
                fitting = exact;
            }
        }
        // A number nothing's decided yet, maybe behind a `&` or in an optional, fits each
        // number type's: it's the one it would be anyway, an `i32` or `f64`. `println(5)` is
        // `println(string::from(5 as i32))`.
        if fitting.len() > 1 {
            fitting.retain(|&(_, arg, _)| self.fits_as_default(arg, shallow));
        }
        Some((from, source, fitting))
    }
}
