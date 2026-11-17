use som_ast::{Expr, Id};
use som_base::msg;
use som_lang::LangTrait;
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::impls::{self, find_impl};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

impl Checker<'_> {
    /// For operator `op` between values of type `ty`: whether it's a trait's method, `Eq`'s
    /// `eq` for `==` or `Add`'s `add` for `+`, recording it for `expr` if so. Gives what it
    /// gives: a `bool` for `==`, what the implementation says for `+`.
    pub(super) fn trait_op(
        &mut self,
        expr: Id<Expr>,
        op: som_ast::BinOp,
        ty: TypeId,
    ) -> Option<TypeId> {
        let lang = op.lang_trait()?;
        let shallow = self.u.shallow(ty);
        // Numbers' own `+`; an unknown, once it's known.
        if self.has_builtin(shallow, lang) || matches!(self.u.get(shallow), Ty::Var(_) | Ty::Error)
        {
            return None;
        }
        let output = self.self_operator(shallow, lang)?;
        self.records.trait_ops.insert(expr, lang);
        if self.operator_generics(lang) {
            self.type_args.insert(expr, vec![shallow, shallow, output]);
        }
        Some(output)
    }

    /// Whether trait `lang` is an operator's taking what's on the right and giving what comes
    /// out as its parameters, `Add<Other = Self, Output = Self>`, as the standard library has
    /// them.
    fn operator_generics(&self, lang: LangTrait) -> bool {
        lang.operator_method().is_some()
            && self
                .names
                .lang(lang)
                .is_some_and(|t| match self.names.def(t).kind {
                    DefKind::Trait(t) => self.ast.traits[t].generics.len() == 2,
                    _ => false,
                })
    }

    /// What operator trait `lang` gives for two values of `ty`, if `ty` has it with itself on
    /// the right: `Add<Money>` for `Money`, or a type parameter required to have it.
    pub(super) fn self_operator(&mut self, ty: TypeId, lang: LangTrait) -> Option<TypeId> {
        let trait_ = self.names.lang(lang)?;
        if !self.operator_generics(lang) {
            return self.implements(ty, trait_).then_some(ty);
        }
        match self.u.get(ty) {
            // `T: Add` is `T: Add<T, T>`.
            Ty::Param(def) => self.names.bounds(*def).contains(&trait_).then_some(ty),
            _ => {
                let u = &self.u;
                let (index, args) =
                    find_impl(&self.impls, &u.table, trait_, ty, &[ty], &|t| u.shallow(t))?;
                let output = *self.impls[index].trait_args.get(1)?;
                let subst = self.impls[index].subst(&args);
                Some(impls::substitute(&mut self.u.table, output, &subst))
            }
        }
    }

    /// `lhs op rhs` through an implementation of operator trait `lang` for `lhs` whose
    /// `Other` fits `rhs`: `impl Multiply<usize> for string` for `"ab" * 3`. Records it for
    /// `expr`, with the trait's type arguments, and gives its `Output`.
    pub(super) fn operator_impl(
        &mut self,
        expr: Id<Expr>,
        lang: LangTrait,
        lhs: TypeId,
        rhs: TypeId,
    ) -> Option<TypeId> {
        if !self.operator_generics(lang) {
            return None;
        }
        let trait_ = self.names.lang(lang)?;
        let base = self.u.shallow(lhs);
        if self.has_builtin(base, lang)
            || matches!(self.u.get(base), Ty::Var(_) | Ty::Error | Ty::Param(_))
        {
            return None;
        }
        let rhs = self.u.shallow(rhs);
        let mut fitting = Vec::new();
        for index in 0..self.impls.len() {
            let info = &self.impls[index];
            let (Some(target), &[other, output]) = (info.target, &info.trait_args[..]) else {
                continue;
            };
            if info.trait_ != Some(trait_) {
                continue;
            }
            let subst = self.fresh_subst(&info.params.clone());
            let [target, other, output] =
                [target, other, output].map(|ty| impls::substitute(&mut self.u.table, ty, &subst));
            if self.u.could_fit(target, base) && self.u.could_fit(other, rhs) {
                fitting.push((target, other, output, subst));
            }
        }
        // An integer nothing's decided yet fits each integer type's: it's the one it would be
        // anyway, an `i32`, unless only one takes it.
        if fitting.len() > 1 {
            fitting.retain(|(_, other, ..)| self.fits_as_default(*other, rhs));
        }
        let (target, other, output, bounded) = fitting.into_iter().next()?;
        for (param, fresh) in bounded {
            for &bound in self.names.bounds(param) {
                self.deferred
                    .obligations
                    .push((fresh, bound, self.span(expr)));
            }
        }
        if self.u.unify(target, base).is_err() || self.u.unify(other, rhs).is_err() {
            return None;
        }
        self.records.trait_ops.insert(expr, lang);
        self.type_args.insert(expr, vec![base, other, output]);
        Some(output)
    }

    /// For trait `trait_`'s method called on `base`, with type arguments `type_args`
    /// (`Self` first): the trait's own arguments the call didn't decide, from the one
    /// implementation for `base` that fits, or else from the trait's defaults. `7.multiply(&3)`
    /// gives what `Multiply<i32>`'s `Output` is, `Self`.
    pub(super) fn settle_trait_args(&mut self, trait_: DefId, base: TypeId, type_args: &[TypeId]) {
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            return;
        };
        let count = self.ast.traits[t].generics.len();
        let Some(args) = type_args.get(1..1 + count) else {
            return;
        };
        if count == 0 {
            return;
        }
        let base = self.u.shallow(base);
        // A known type's implementation decides, if it has one: numbers' built-in
        // operators have none, and get the defaults.
        if !matches!(self.u.get(base), Ty::Var(_) | Ty::Param(_) | Ty::Error) {
            let mut fitting = Vec::new();
            for info in &self.impls {
                let Some(target) = info.target.filter(|_| info.trait_ == Some(trait_)) else {
                    continue;
                };
                if info.trait_args.len() != count {
                    continue;
                }
                fitting.push((info.params.clone(), target, info.trait_args.clone()));
            }
            let mut found = Vec::new();
            for (params, target, written) in fitting {
                let subst = self.fresh_subst(&params);
                let target = impls::substitute(&mut self.u.table, target, &subst);
                let written: Vec<TypeId> = (written.into_iter())
                    .map(|w| impls::substitute(&mut self.u.table, w, &subst))
                    .collect();
                let fits = self.u.could_fit(target, base)
                    && (written.iter().zip(args)).all(|(&w, &a)| self.u.could_fit(w, a));
                if fits {
                    found.push((target, written));
                }
            }
            if let [(target, written)] = &found[..] {
                let _ = self.u.unify(*target, base);
                for (&w, &a) in written.iter().zip(args) {
                    let _ = self.u.unify(w, a);
                }
                return;
            }
        }
        // What nothing decided is the default: `Output = Self`.
        let generics: Vec<_> = self.ast.traits[t]
            .generics
            .iter()
            .map(|g| g.default)
            .collect();
        let params = self.names.trait_params(t).to_vec();
        for (index, &arg) in args.iter().enumerate() {
            let arg = self.u.shallow(arg);
            let undecided = self.u.var_kind(arg) == Some(VarKind::Any);
            let Some(default) = generics[index].filter(|_| undecided) else {
                continue;
            };
            let ty = self.lower(default);
            let subst: Vec<(DefId, TypeId)> = params
                .iter()
                .copied()
                .zip(std::iter::once(base).chain(args.iter().copied()))
                .collect();
            let ty = self.substitute(ty, &subst);
            let _ = self.u.unify(arg, ty);
        }
    }

    /// For an operator values of `ty` don't have: a fix giving it them, through trait `lang`.
    /// For a struct, an implementation after it; for a type parameter, the bound.
    /// With `other`, what's on the right of the operator, when it's not `ty` itself: the
    /// implementation takes it, `impl Multiply<f64> for Vector`.
    pub(super) fn operator_fix(
        &self,
        ty: TypeId,
        lang: LangTrait,
        other: Option<TypeId>,
    ) -> Option<som_base::Fix> {
        let trait_ = self.names.lang(lang)?;
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            return None;
        };
        let trait_name = lang.name();
        match self.u.shallow_ty(ty).clone() {
            // `fn f<T>` gets `<T: Compare>`, `<T: Eq>` gets `<T: Eq + Compare>`.
            Ty::Param(def) => {
                let DefKind::TypeParam { func, index, .. } = self.names.def(def).kind else {
                    return None;
                };
                let generic = self.ast.funcs[func].generics.get(index)?;
                let (end, text) = match generic.bounds.last() {
                    Some(last) => (last.span(), format!(" + {trait_name}")),
                    None => (generic.name.span.clone(), format!(": {trait_name}")),
                };
                let at = end.after();
                let title = msg![
                    "require ",
                    som_base::message::code(format!("{}: {trait_name}", generic.name.name))
                ];
                Some(som_base::Fix {
                    title,
                    edits: vec![(at, text)],
                })
            }
            // After `struct Hello { … }`: `impl Compare for Hello { … }`, with its methods.
            Ty::Struct(owner, _) => {
                let DefKind::Struct(s) = self.names.def(owner).kind else {
                    return None;
                };
                let span = self.ast.structs.span(s);
                // Not in the standard library, which can't be changed from here.
                if self.in_std(owner) {
                    return None;
                }
                let st = &self.ast.structs[s];
                let params: Vec<&str> = st.generics.iter().map(|g| &*g.name.name).collect();
                let (generics, target) = match params.is_empty() {
                    true => (String::new(), st.name.name.to_string()),
                    false => (
                        format!("<{}>", params.join(", ")),
                        format!("{}<{}>", st.name.name, params.join(", ")),
                    ),
                };
                // `Other` and `Output`: what's on the right, and `Self`.
                let shown = |ty: TypeId| {
                    // A number nothing decided is the `i32` or `f64` it would be.
                    let ty = match self.u.var_kind(self.u.shallow(ty)) {
                        Some(VarKind::Int) => self.u.table.i32,
                        Some(VarKind::Float) => self.u.table.f64,
                        Some(VarKind::Text) => self.u.table.text,
                        _ => ty,
                    };
                    self.u.display(ty, self.names)
                };
                let other = other
                    .map(shown)
                    .filter(|o| *o != target && self.operator_generics(lang));
                let trait_args: Vec<String> = match &other {
                    Some(other) => vec![other.clone(), target.clone()],
                    None if self.operator_generics(lang) => vec![target.clone(), target.clone()],
                    None => Vec::new(),
                };
                let stubs = crate::stubs::stubs(self.ast, t, &target, &trait_args, &|_, _| false);
                let body: String = stubs.iter().map(|s| s.text.as_str()).collect();
                let written = match &other {
                    Some(other) => format!("{trait_name}<{other}>"),
                    None => trait_name.to_string(),
                };
                let text = format!("\n\nimpl{generics} {written} for {target} {{\n{body}}}");
                let title = msg![
                    "implement ",
                    som_base::message::code(written),
                    " for ",
                    som_base::message::code(target.clone())
                ];
                Some(som_base::Fix {
                    title,
                    edits: vec![(span.after(), text)],
                })
            }
            _ => None,
        }
    }

    /// What operator trait `lang`'s implementations for `ty` take on the right, as written:
    /// `usize` for `string`'s `*`.
    pub(super) fn operator_takes(&self, ty: TypeId, lang: LangTrait) -> Vec<String> {
        let Some(trait_) = self
            .names
            .lang(lang)
            .filter(|_| self.operator_generics(lang))
        else {
            return Vec::new();
        };
        let ty = self.u.shallow(ty);
        let mut takes = Vec::new();
        for info in &self.impls {
            let (Some(target), Some(&other)) = (info.target, info.trait_args.first()) else {
                continue;
            };
            if info.trait_ != Some(trait_) || !self.u.could_fit(target, ty) {
                continue;
            }
            // `Self` on the right is the type itself.
            let other = match other == target {
                true => ty,
                false => other,
            };
            let shown = self.u.display(other, self.names);
            if !takes.contains(&shown) {
                takes.push(shown);
            }
        }
        takes
    }
}
