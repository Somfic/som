//! Traits in the type checker: impl blocks and the types they name (`Self`, `Item`), calls of
//! trait methods, the bounds type parameters have to meet, and the traits the language itself
//! uses (`LangTrait`).

use som_ast::{Expr, Func, Id, Item, Span};
use som_base::msg;
use som_resolve::{DefId, DefKind, LangTrait};

use super::Checker;
use crate::error::TypeError;
use crate::traits::{self, ImplInfo, find_impl};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

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
            let target = match block.target_ty {
                Some(ty) => Some(self.lower(ty)),
                None => def.owner.and_then(|owner| match self.names.def(owner).kind {
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
                }),
            };
            let trait_args: Vec<TypeId> = (block.trait_.iter())
                .flat_map(|(_, args)| args)
                .map(|&ty| self.lower(ty))
                .collect();
            let written: Vec<TypeId> = block.types.iter().map(|&(_, ty)| self.lower(ty)).collect();
            let mut types = Vec::new();
            if let Some(trait_) = trait_
                && let DefKind::Trait(t) = self.names.def(trait_).kind
            {
                let trait_name = self.names.def(trait_).name.to_string();
                // Missing ones are reported with the methods, in `check_impls`.
                for &assoc in self.names.trait_types(t) {
                    let name = &self.names.def(assoc).name;
                    let found = block.types.iter().position(|(n, _)| n.name == *name);
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
                trait_args,
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
            // A copied value would be dropped once for each copy.
            if self.names.lang_of(trait_) == Some(LangTrait::Drop)
                && let Some(target) = self.impls[index].target
                && self.is_copy(target)
            {
                self.errors.push(TypeError::DropOnCopy {
                    ty: self.describe(target),
                    span: self.impl_target_span(index),
                });
            }
            // `impl Add for i32` would change what `1 + 2` means.
            if let Some(lang) = self.names.lang_of(trait_)
                && let Some(target) = self.impls[index].target
                && self.has_builtin(target, lang)
            {
                self.errors.push(TypeError::BuiltinImpl {
                    ty: self.describe(target),
                    trait_: trait_name.clone(),
                    span: self.impl_target_span(index),
                });
            }
            let wanted = self.ast.traits[t].methods.clone();
            let written = self.impls[index].methods.clone();
            for &method in &wanted {
                let name = &self.ast.funcs[method].name.name;
                let found = written
                    .iter()
                    .find(|&&m| self.ast.funcs[m].name.name == *name);
                if let Some(&m) = found {
                    self.check_impl_method(index, method, m);
                }
            }
            self.report_missing(index, &trait_name);
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

    /// Reports the associated types and methods impl block `index` doesn't write, with a fix
    /// writing them, at the trait it names.
    fn report_missing(&mut self, index: usize, trait_name: &str) {
        let block = impl_block(self.ast, index);
        let missing = crate::stubs::missing(self.ast, self.names, index, block);
        if missing.is_empty() {
            return;
        }
        let span = (block.trait_.as_ref()).map_or(block.target.span(), |(path, _)| path.span());
        // Each on lines of its own, before the closing brace, in place of the spaces there.
        let close = &block.close;
        let before = &close.source.content()[..close.start_offset];
        let kept = before.trim_end_matches([' ', '\t']);
        let opener = if kept.ends_with('\n') { "" } else { "\n" };
        let text: String = missing.iter().map(|m| m.text.as_str()).collect();
        let at = Span::from_range(kept.len()..close.start_offset, close.source.clone());
        self.errors.push(TypeError::MissingTraitItem {
            missing: missing.iter().map(|m| m.signature.clone()).collect(),
            trait_: trait_name.to_string(),
            span,
            fix: (at, format!("{opener}{text}")),
        });
    }

    /// Where impl block `index` names what it's for.
    fn impl_target_span(&self, index: usize) -> Span {
        impl_block(self.ast, index).target.span()
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
        // The trait's own parameters come after `Self`: what the implementation says they are.
        let trait_args = self.impls[index].trait_args.clone();
        for (&param, &arg) in type_args.iter().skip(1).zip(&trait_args) {
            let arg = self.in_method_terms(index, arg);
            let _ = self.u.unify(param, arg);
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
            find_impl(&self.impls, &u.table, trait_, base, &[], &|t| u.shallow(t))
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
        if let Some(lang) = self.names.lang_of(trait_)
            && self.has_builtin(ty, lang)
        {
            return true;
        }
        match self.u.get(ty) {
            Ty::Error => true,
            Ty::Param(def) => self.names.bounds(*def).contains(&trait_),
            _ => {
                let u = &self.u;
                find_impl(&self.impls, &u.table, trait_, ty, &[], &|t| u.shallow(t)).is_some()
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
    /// methods; on a struct without a method of that name, a trait's it implements; on any
    /// other type, a trait's it implements or has built in, as numbers have `compare`. Returns
    /// the call's type, if it's one.
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
        let implemented = |c: &Self| -> Vec<DefId> {
            (c.impls.iter())
                .filter_map(|info| info.trait_)
                .filter(|&t| c.implements(base, t))
                .collect()
        };
        let traits: Vec<DefId> = match self.u.get(base).clone() {
            Ty::Param(def) => self.names.bounds(def).to_vec(),
            Ty::Struct(owner, _) if self.names.method(owner, &method.name).is_none() => {
                implemented(self)
            }
            Ty::Struct(..) | Ty::Error => return None,
            // A number not settled yet has the numbers' built-in traits.
            Ty::Var(_) => match self.u.var_kind(base) {
                Some(VarKind::Int | VarKind::Float) => [
                    LangTrait::Add,
                    LangTrait::Subtract,
                    LangTrait::Multiply,
                    LangTrait::Divide,
                    LangTrait::Compare,
                    LangTrait::Eq,
                ]
                .into_iter()
                .filter_map(|l| self.names.lang(l))
                .collect(),
                _ => return None,
            },
            _ => {
                let mut traits = implemented(self);
                traits.extend(
                    (LangTrait::ALL.into_iter())
                        .filter(|&l| self.has_builtin(base, l))
                        .filter_map(|l| self.names.lang(l)),
                );
                traits
            }
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

    /// For operator `op` between values of type `ty`: whether it's a trait's method, `Eq`'s
    /// `eq` for `==` or `Add`'s `add` for `+`, recording it for `expr` if so.
    pub(super) fn trait_op(&mut self, expr: Id<Expr>, op: som_ast::BinOp, ty: TypeId) -> bool {
        let Some(lang) = LangTrait::of_operator(op) else {
            return false;
        };
        let shallow = self.u.shallow(ty);
        // Numbers' own `+`; an unknown, once it's known.
        if self.has_builtin(shallow, lang)
            || matches!(self.u.get(shallow), Ty::Var(_) | Ty::Error)
            || !self.implements_lang(shallow, lang)
        {
            return false;
        }
        self.trait_ops.insert(expr, lang);
        true
    }

    /// For an operator values of `ty` don't have: a fix giving it them, through trait `lang`.
    /// For a struct, an implementation after it; for a type parameter, the bound.
    pub(super) fn operator_fix(&self, ty: TypeId, lang: LangTrait) -> Option<som_base::Fix> {
        let trait_ = self.names.lang(lang)?;
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            return None;
        };
        let trait_name = lang.name();
        match self.u.get(self.u.shallow(ty)).clone() {
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
                if span.source.identifier().starts_with('<') {
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
                let stubs = crate::stubs::stubs(self.ast, t, &target, &[], &|_, _| false);
                let body: String = stubs.iter().map(|s| s.text.as_str()).collect();
                let text =
                    format!("\n\nimpl{generics} {trait_name} for {target} {{\n{body}}}");
                let title = msg![
                    "implement ",
                    som_base::message::code(trait_name),
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

    /// Whether values of `ty` have the operators of `lang` built in, as numbers have `+`.
    pub(super) fn has_builtin(&self, ty: TypeId, lang: LangTrait) -> bool {
        traits::builtin_operator(self.u.get(self.u.shallow(ty)), lang)
    }

    /// Whether `expr`, of type `found`, becomes the struct `expected` through its `From`
    /// implementation: `let list: List<i32> = [1, 2, 3]` is `List::from([1, 2, 3])`. Records it
    /// if so. Only one step, only into a struct that's known, and only when exactly one
    /// implementation could take `found`; the struct's type arguments can follow from it.
    pub(super) fn from_conversion(&mut self, expr: Id<Expr>, expected: TypeId, found: TypeId) -> bool {
        let Some((from, source, fitting)) = self.from_impls(expected, found) else {
            return false;
        };
        let [(target, arg)] = fitting[..] else {
            return false;
        };
        let (expected, shallow) = (self.u.shallow(expected), self.u.shallow(found));
        if self.u.unify(target, expected).is_err() || self.u.unify(arg, source).is_err() {
            return false;
        }
        if source != shallow {
            let _ = self.u.unify(shallow, source);
        }
        let Some(func) = self.trait_method(from, "from") else {
            return false;
        };
        self.conversions
            .insert(expr, (self.names.func(func), vec![expected, source]));
        true
    }

    /// Whether a `found` would become the struct `expected` through `From`, without changing
    /// anything. For choosing between functions with the same name.
    pub(super) fn could_convert(&mut self, expected: TypeId, found: TypeId) -> bool {
        self.from_impls(expected, found)
            .is_some_and(|(.., fitting)| fitting.len() == 1)
    }

    /// The `From` implementations that could make a `found` into the struct `expected`, each
    /// as its target and source with its parameters fresh unknowns; with the trait, and what
    /// `found` counts as (a collection literal as an array). `None` if `expected` isn't a
    /// struct or `found` isn't known well enough to say.
    fn from_impls(
        &mut self,
        expected: TypeId,
        found: TypeId,
    ) -> Option<(DefId, TypeId, Vec<(TypeId, TypeId)>)> {
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
            let params = info.params.clone();
            let fresh: Vec<TypeId> = params.iter().map(|_| self.u.fresh(VarKind::Any)).collect();
            let target = traits::substitute(&mut self.u.table, target, &params, &fresh);
            let arg = traits::substitute(&mut self.u.table, arg, &params, &fresh);
            if self.u.could_fit(target, expected) && self.u.could_fit(arg, source) {
                fitting.push((target, arg));
            }
        }
        Some((from, source, fitting))
    }
}

/// Impl block `index`, counting them in every module in order, as `Names::impls` does.
fn impl_block(ast: &som_ast::Ast, index: usize) -> &som_ast::Impl {
    (ast.modules.iter())
        .flat_map(|m| &m.items)
        .filter_map(|item| match item {
            Item::Impl(block) => Some(block),
            _ => None,
        })
        .nth(index)
        .expect("an impl block for each index")
}
