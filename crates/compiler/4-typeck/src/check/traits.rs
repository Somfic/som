//! Traits in the type checker: impl blocks and the types they name (`Self`, `Item`), calls of
//! trait methods, the bounds type parameters have to meet, and the traits the language itself
//! uses (`LangTrait`).

use som_ast::{Expr, Func, Id, Item, Span};
use som_resolve::{DefId, DefKind, LangTrait};

use super::Checker;
use crate::error::TypeError;
use crate::traits::{self, ImplInfo, find_impl};
use crate::ty::{Ty, TypeId};

impl Checker<'_> {
    // --- Impl blocks ---

    /// Lowers every impl block's target and associated types, before any signature uses
    /// `Self` or `Item`. Reports associated types a trait has that an implementation doesn't
    /// say, or says that the trait doesn't have.
    pub(super) fn build_impls(&mut self) {
        let ast = self.ast;
        let blocks = ast
            .modules
            .iter()
            .flat_map(|m| &m.items)
            .filter_map(|item| match item {
                Item::Impl(block) => Some(block),
                _ => None,
            });
        for (index, block) in blocks.enumerate() {
            let def = &self.names.impls[index];
            let params = def.params.clone();
            let trait_ = def.trait_;
            let target = def
                .owner
                .and_then(|owner| match self.names.def(owner).kind {
                    DefKind::Struct(s) => {
                        // `impl<T> Pair<T>` written as `impl Pair`: its parameters, in order.
                        let expected = self.names.struct_params(s).len();
                        let args = if block.target_args.is_empty() && block.trait_.is_none() {
                            (0..expected)
                                .map(|i| match params.get(i) {
                                    Some(&p) => self.u.intern(Ty::Param(p)),
                                    None => self.u.table.error,
                                })
                                .collect()
                        } else {
                            self.lower_args(owner, s, &block.target_args, block.target.span())
                        };
                        Some(self.u.intern(Ty::Struct(owner, args)))
                    }
                    _ => None,
                });
            let written: Vec<TypeId> = block.types.iter().map(|&(_, ty)| self.lower(ty)).collect();
            let mut types = Vec::new();
            if let Some(trait_) = trait_
                && let DefKind::Trait(t) = self.names.def(trait_).kind
            {
                let trait_name = self.names.def(trait_).name.to_string();
                for &assoc in self.names.trait_types(t) {
                    let name = &self.names.def(assoc).name;
                    let found = block.types.iter().position(|(n, _)| n.name == *name);
                    if found.is_none() {
                        self.errors.push(TypeError::MissingTraitItem {
                            what: "type",
                            name: name.to_string(),
                            trait_: trait_name.clone(),
                            span: block.target.span(),
                        });
                    }
                    types.push(found.map(|i| written[i]));
                }
                for (name, _) in &block.types {
                    let known = (self.names.trait_types(t).iter())
                        .any(|&d| self.names.def(d).name == name.name);
                    if !known {
                        self.errors.push(TypeError::NotInTrait {
                            what: "type",
                            name: name.name.to_string(),
                            trait_: trait_name.clone(),
                            span: name.span.clone(),
                        });
                    }
                }
            }
            self.impls.push(ImplInfo {
                trait_,
                params,
                target,
                types,
                written_types: written,
                methods: block.methods.clone(),
            });
        }
    }

    /// Checks each implementation of a trait has its methods, with the types it says, and no
    /// others.
    pub(super) fn check_impls(&mut self) {
        for index in 0..self.impls.len() {
            let Some(trait_) = self.impls[index].trait_ else {
                continue;
            };
            let DefKind::Trait(t) = self.names.def(trait_).kind else {
                continue;
            };
            let trait_name = self.names.def(trait_).name.to_string();
            let wanted = self.ast.traits[t].methods.clone();
            let written = self.impls[index].methods.clone();
            for &method in &wanted {
                let name = &self.ast.funcs[method].name.name;
                let found = written
                    .iter()
                    .find(|&&m| self.ast.funcs[m].name.name == *name);
                match found {
                    Some(&m) => self.check_impl_method(index, method, m),
                    // A method with a body is what implementations get unless they write one.
                    None if self.ast.funcs[method].body.is_some() => {}
                    None => {
                        let span = self.names.impls[index]
                            .owner
                            .map(|o| self.names.def(o).span.clone())
                            .unwrap_or_else(|| self.ast.funcs.span(method).clone());
                        self.errors.push(TypeError::MissingTraitItem {
                            what: "method",
                            name: name.to_string(),
                            trait_: trait_name.clone(),
                            span,
                        });
                    }
                }
            }
            for &m in &written {
                let name = &self.ast.funcs[m].name;
                if !wanted
                    .iter()
                    .any(|&w| self.ast.funcs[w].name.name == name.name)
                {
                    self.errors.push(TypeError::NotInTrait {
                        what: "method",
                        name: name.name.to_string(),
                        trait_: trait_name.clone(),
                        span: name.span.clone(),
                    });
                }
            }
        }
    }

    /// Checks impl method `written`, of impl block `index`, has the types trait method
    /// `wanted` says, with `Self` its target and the associated types it says.
    fn check_impl_method(&mut self, index: usize, wanted: Id<Func>, written: Id<Func>) {
        let outer = self.current_func.replace(written);
        let (sig, type_args) = self.instantiate(wanted);
        if let (Some(&this), Some(target)) = (type_args.first(), self.impls[index].target) {
            let target = self.in_method_terms(index, target);
            let _ = self.u.unify(this, target);
        }
        let (params, ret): (Vec<TypeId>, TypeId) = (
            sig.params
                .iter()
                .map(|&p| self.normalize_assoc(p))
                .collect(),
            self.normalize_assoc(sig.ret),
        );
        let actual = self.sigs[written.id].clone();
        let fits = params.len() == actual.params.len()
            && (params.iter().zip(&actual.params)).all(|(&p, &a)| self.u.unify(p, a).is_ok())
            && self.u.unify(ret, actual.ret).is_ok();
        if !fits {
            let shown: Vec<String> = (params.iter())
                .map(|&p| self.u.display(p, self.names))
                .collect();
            let name = self.ast.funcs[written].name.clone();
            let ret = match self.u.get(self.u.shallow(ret)) {
                Ty::Unit => String::new(),
                _ => format!(" -> {}", self.u.display(ret, self.names)),
            };
            self.errors.push(TypeError::ImplSignatureMismatch {
                name: name.name.to_string(),
                expected: format!("fn {}({}){ret}", name.name, shown.join(", ")),
                span: name.span.clone(),
                declared_at: self.ast.funcs[wanted].name.span.clone(),
            });
        }
        self.current_func = outer;
    }

    /// `ty`, written in terms of impl block `index`'s parameters, in terms of the method being
    /// checked's own copies of them, if it's one of that block's.
    pub(super) fn in_method_terms(&mut self, index: usize, ty: TypeId) -> TypeId {
        let Some(func) = self.current_func else {
            return ty;
        };
        if self.names.impl_of(func) != Some(index) {
            return ty;
        }
        let params = self.impls[index].params.clone();
        let args: Vec<TypeId> = (self.names.type_params(func).iter().take(params.len()))
            .map(|&p| self.u.intern(Ty::Param(p)))
            .collect();
        if args.len() != params.len() {
            return ty;
        }
        traits::substitute(&mut self.u.table, ty, &params, &args)
    }

    /// `Self` inside impl block `index`: its target.
    pub(super) fn self_type(&mut self, index: usize) -> TypeId {
        match self.impls.get(index).and_then(|i| i.target) {
            Some(target) => self.in_method_terms(index, target),
            None => self.u.table.error,
        }
    }

    /// The associated type impl block `index` says `index_in_block`th, as written.
    pub(super) fn impl_type(&mut self, index: usize, written: usize) -> TypeId {
        match self
            .impls
            .get(index)
            .and_then(|i| i.written_types.get(written))
            .copied()
        {
            Some(ty) => self.in_method_terms(index, ty),
            None => self.u.table.error,
        }
    }

    /// An associated type named in a type: `Item` inside its trait (of `Self`), or `T::Item`
    /// (of `base`).
    pub(super) fn assoc_type(
        &mut self,
        base: Option<DefId>,
        trait_: som_ast::Id<som_ast::Trait>,
        index: usize,
    ) -> TypeId {
        let base = match base.map(|b| (b, &self.names.def(b).kind)) {
            Some((_, &DefKind::SelfType { impl_ })) => self.self_type(impl_),
            Some((b, _)) => self.u.intern(Ty::Param(b)),
            // Inside the trait: of `Self`, its methods' first generic parameter.
            None => match self
                .current_func
                .and_then(|f| self.names.type_params(f).first())
            {
                Some(&this) => self.u.intern(Ty::Param(this)),
                None => return self.u.table.error,
            },
        };
        let trait_ = self.names.trait_def(trait_);
        let assoc = self.u.intern(Ty::Assoc {
            base,
            trait_,
            index,
        });
        self.normalize_assoc(assoc)
    }

    /// `ty` with every associated type of a type that's known worked out through its
    /// implementation: `ListIter<i32>::Item` is `&i32`. One of a type parameter, or of a type
    /// not known yet, stays as it is.
    pub(super) fn normalize_assoc(&mut self, ty: TypeId) -> TypeId {
        let ty = self.u.shallow(ty);
        let t = self.u.get(ty).clone();
        if t.children().is_empty() {
            return ty;
        }
        let inner = t.map_children(|child| self.normalize_assoc(child));
        let inner = self.u.intern(inner);
        let Ty::Assoc {
            base,
            trait_,
            index,
        } = *self.u.get(inner)
        else {
            return inner;
        };
        let found = {
            let u = &self.u;
            find_impl(&self.impls, &u.table, trait_, base, &|t| u.shallow(t))
        };
        let Some((i, args)) = found else {
            return inner;
        };
        let Some(written) = self.impls[i].types.get(index).copied().flatten() else {
            return self.u.table.error;
        };
        let params = self.impls[i].params.clone();
        let found = traits::substitute(&mut self.u.table, written, &params, &args);
        self.normalize_assoc(found)
    }

    // --- What types implement ---

    /// Whether `ty` implements trait `trait_`: a type parameter if it's one of its bounds,
    /// anything else if an `impl` says so.
    pub(super) fn implements(&self, ty: TypeId, trait_: DefId) -> bool {
        let ty = self.u.shallow(ty);
        match self.u.get(ty) {
            Ty::Error => true,
            Ty::Param(def) => self.names.bounds(*def).contains(&trait_),
            _ => {
                let u = &self.u;
                find_impl(&self.impls, &u.table, trait_, ty, &|t| u.shallow(t)).is_some()
            }
        }
    }

    /// Whether `ty` implements the language's trait `lang`, if the standard library has it.
    pub(super) fn implements_lang(&self, ty: TypeId, lang: LangTrait) -> bool {
        self.names
            .lang(lang)
            .is_some_and(|t| self.implements(ty, t))
    }

    /// Remembers that the type arguments `args` given to `func`'s generic parameters have to
    /// implement their bounds, to check once they're known.
    pub(super) fn require_bounds(&mut self, func: Id<Func>, args: &[TypeId], span: Span) {
        let params = self.names.type_params(func);
        for (&param, &arg) in params.iter().zip(args) {
            for &trait_ in self.names.bounds(param) {
                self.obligations.push((arg, trait_, span.clone()));
            }
        }
    }

    /// Reports type arguments that turned out not to implement what their parameter needs.
    pub(super) fn check_obligations(&mut self) {
        for (ty, trait_, span) in std::mem::take(&mut self.obligations) {
            let ty = self.normalize_assoc(ty);
            if self.u.var_kind(ty).is_some() || self.implements(ty, trait_) {
                continue;
            }
            self.errors.push(TypeError::DoesNotImplement {
                ty: self.describe(ty),
                trait_: self.names.def(trait_).name.to_string(),
                span,
            });
        }
    }

    // --- Trait methods ---

    /// Trait `trait_`'s method called `name`.
    pub(super) fn trait_method(&self, trait_: DefId, name: &str) -> Option<Id<Func>> {
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            return None;
        };
        (self.ast.traits[t].methods.iter().copied())
            .find(|&m| &*self.ast.funcs[m].name.name == name)
    }

    /// The index of trait `trait_`'s associated type `name`.
    pub(super) fn assoc_index(&self, trait_: DefId, name: &str) -> Option<usize> {
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            return None;
        };
        (self.names.trait_types(t).iter()).position(|&d| &*self.names.def(d).name == name)
    }

    /// `receiver.method(args)` of a trait's method: on a type parameter, one of its bounds'
    /// methods; on a struct without a method of that name, a method with a body of a trait it
    /// implements. Returns the call's type, if it's one.
    pub(super) fn trait_method_call(
        &mut self,
        call: Id<Expr>,
        receiver: Id<Expr>,
        receiver_ty: TypeId,
        method: &som_ast::Ident,
        args: &[Id<Expr>],
    ) -> Option<TypeId> {
        let mut base = self.u.shallow(receiver_ty);
        if let Ty::Ref { to, .. } = *self.u.get(base) {
            base = self.u.shallow(to);
        }
        let traits: Vec<DefId> = match self.u.get(base).clone() {
            Ty::Param(def) => self.names.bounds(def).to_vec(),
            Ty::Struct(owner, _) if self.names.method(owner, &method.name).is_none() => {
                (self.impls.iter())
                    .filter_map(|info| info.trait_)
                    .filter(|&t| self.implements(base, t))
                    .collect()
            }
            _ => return None,
        };
        let func = traits
            .iter()
            .find_map(|&t| self.trait_method(t, &method.name))?;
        let (sig, type_args) = self.instantiate(func);
        let _ = self.u.unify(type_args[0], base);
        let params: Vec<TypeId> = sig
            .params
            .iter()
            .map(|&p| self.normalize_assoc(p))
            .collect();
        let ret = self.normalize_assoc(sig.ret);
        self.methods.insert(call, self.names.func(func));
        self.type_args.insert(call, type_args.clone());
        self.require_bounds(func, &type_args, self.span(call));
        let name = format!("{}::{}", self.u.display(base, self.names), method.name);
        match params.split_first() {
            Some((&self_ty, rest)) => {
                if !self.receive(receiver, self_ty, receiver_ty) {
                    self.mismatch(self_ty, receiver_ty, self.span(receiver), None);
                }
                self.check_args_from(call, &name, func, 1, rest, args);
            }
            None => {
                self.errors.push(TypeError::WrongArgCount {
                    name,
                    expected: 0,
                    at_least: false,
                    found: args.len() + 1,
                    span: self.span(call),
                });
            }
        }
        Some(ret)
    }

    // --- The language's traits ---

    /// What a `for` loop over `iterable`, of type `ty`, binds each time round, if it's an
    /// `Iterator` or an `Iterable`. Records how it goes through it.
    pub(super) fn iterator_loop(&mut self, stmt: Id<som_ast::Stmt>, ty: TypeId) -> Option<TypeId> {
        let iterator = self.names.lang(LangTrait::Iterator)?;
        let item = self.assoc_index(iterator, "Item")?;
        let shallow = self.u.shallow(ty);
        let viewed = match *self.u.get(shallow) {
            Ty::Ref { to, .. } => self.u.shallow(to),
            _ => shallow,
        };
        // An iterator itself, by value: `for x in list.iter()`.
        let (iter_ty, iterable) = if !matches!(self.u.get(shallow), Ty::Ref { .. })
            && self.implements(shallow, iterator)
        {
            (shallow, false)
        } else {
            let iterable = self.names.lang(LangTrait::Iterable)?;
            if !self.implements(viewed, iterable) {
                return None;
            }
            let iter = self.assoc_index(iterable, "Iter")?;
            let assoc = self.u.intern(Ty::Assoc {
                base: viewed,
                trait_: iterable,
                index: iter,
            });
            (self.normalize_assoc(assoc), true)
        };
        let assoc = self.u.intern(Ty::Assoc {
            base: iter_ty,
            trait_: iterator,
            index: item,
        });
        let element = self.normalize_assoc(assoc);
        self.for_loops
            .insert(stmt, crate::Loop::Iterator { iterable });
        Some(element)
    }

    /// For `==` or `!=` between values of type `ty`: whether it's `Eq`'s `eq`, recording it
    /// for `expr` if so.
    pub(super) fn eq_op(&mut self, expr: Id<Expr>, ty: TypeId) -> bool {
        let shallow = self.u.shallow(ty);
        if !matches!(self.u.get(shallow), Ty::Struct(..) | Ty::Param(_))
            || !self.implements_lang(shallow, LangTrait::Eq)
        {
            return false;
        }
        self.eq_ops.insert(expr);
        true
    }

    /// Whether collection literal `expr`, of type `found` (`element`s, `len` of them), becomes
    /// `expected` through its `FromArray` implementation. Records it if so.
    pub(super) fn from_array(
        &mut self,
        expr: Id<Expr>,
        expected: TypeId,
        found: TypeId,
        element: TypeId,
        len: TypeId,
    ) -> bool {
        let Some(from_array) = self.names.lang(LangTrait::FromArray) else {
            return false;
        };
        let expected_shallow = self.u.shallow(expected);
        if !matches!(self.u.get(expected_shallow), Ty::Struct(..))
            || !self.implements(expected_shallow, from_array)
        {
            return false;
        }
        let Some(func) = self.trait_method(from_array, "from_array") else {
            return false;
        };
        let (sig, type_args) = self.instantiate(func);
        let _ = self.u.unify(type_args[0], expected);
        let Some(&param) = sig.params.first() else {
            return false;
        };
        let param = self.normalize_assoc(param);
        let array = self.u.intern(Ty::Array { element, len });
        if !self.u.could_fit(param, array) || self.u.unify(param, array).is_err() {
            return false;
        }
        let _ = self.u.unify(found, array);
        self.conversions
            .insert(expr, (self.names.func(func), type_args));
        true
    }
}
