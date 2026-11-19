use som_ast::{Expr, Func, Id, Item, Span};
use som_lang::LangTrait;
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::Start;
use crate::error::TypeError;
use crate::impls::{self, ImplInfo, find_impl};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

impl Checker<'_> {
    /// Lowers every impl block's target and associated types, before any signature uses
    /// `Self` or `Item`.
    pub(super) fn build_impls(&mut self) {
        let ast = self.ast;
        for (index, block) in impl_blocks(ast).enumerate() {
            let def = &self.names.impls[index];
            let params = def.params.clone();
            let trait_ = def.trait_;
            let blanket = def.blanket;
            let target = match (block.target_ty, blanket) {
                // `impl<C: Index> IterRef for C`: for its parameter, whatever it turns out to be.
                (_, Some(param)) => Some(self.u.intern(Ty::Param(param))),
                (Some(ty), None) => Some(self.lower(ty)),
                (None, None) => def
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
                    }),
            };
            let mut trait_args: Vec<TypeId> = (block.trait_.iter())
                .flat_map(|(_, args)| args)
                .map(|&ty| self.lower(ty))
                .collect();
            // `impl Add for Money` is `impl Add<Money, Money> for Money`: what's left out is
            // the trait's default, with `Self` the target.
            if let (Some(trait_), Some((path, _))) = (trait_, &block.trait_)
                && let DefKind::Trait(t) = self.names.def(trait_).kind
            {
                let this = target.unwrap_or(self.u.table.error);
                trait_args = self.trait_args(trait_, t, this, trait_args, path.span());
            }
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
                blanket: blanket.map(|param| self.names.bounds(param).to_vec()),
            });
        }
    }

    pub(super) fn trait_args(
        &mut self,
        trait_: DefId,
        t: som_ast::Id<som_ast::Trait>,
        this: TypeId,
        given: Vec<TypeId>,
        span: Span,
    ) -> Vec<TypeId> {
        let generics = &self.ast.traits[t].generics;
        let defaults: Vec<_> = generics.iter().map(|g| g.default).collect();
        let required = defaults.iter().take_while(|d| d.is_none()).count();
        if given.len() < required || given.len() > generics.len() {
            self.wrong_arg_count(trait_, required, generics.len(), given.len(), span);
            return vec![self.u.table.error; generics.len()];
        }
        // In terms of `Self` first: what the defaults are written in.
        let params = self.names.trait_params(t).to_vec();
        let defaults: Vec<_> = std::iter::once(None).chain(defaults).collect();
        let given = std::iter::once(this).chain(given).collect();
        let mut args = self.with_defaults(&params, &defaults, given);
        args.remove(0);
        args
    }

    pub(super) fn check_impls(&mut self) {
        for index in 0..self.impls.len() {
            let Some(trait_) = self.impls[index].trait_ else {
                continue;
            };
            let DefKind::Trait(t) = self.names.def(trait_).kind else {
                continue;
            };
            let trait_name = self.names.def(trait_).name.to_string();
            // Copying a struct copies its fields: they have to be copied too.
            if self.names.lang_of(trait_) == Some(LangTrait::Copy)
                && let Some(target) = self.impls[index].target
                && let Ty::Struct(def, args) = self.u.get(target).clone()
                && let DefKind::Struct(s) = self.names.def(def).kind
            {
                let subst = self.struct_subst(s, &args);
                let types = self.records.structs[s.id].clone();
                for (field, &ty) in self.ast.structs[s].fields.iter().zip(&types) {
                    let ty = impls::substitute(&mut self.u.table, ty, &subst);
                    if !self.is_copy(ty) && !matches!(self.u.get(ty), Ty::Param(_)) {
                        self.errors.push(TypeError::CopyWithMovedField {
                            ty: self.describe(target),
                            field: field.name.name.to_string(),
                            field_ty: self.describe(ty),
                            span: field.name.span.clone(),
                        });
                    }
                }
            }
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
            // A copied value is copied bit for bit: its `share` would never be called.
            if self.names.lang_of(trait_) == Some(LangTrait::Share)
                && let Some(target) = self.impls[index].target
                && self.is_copy(target)
            {
                self.errors.push(TypeError::ShareOnCopy {
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

    /// Reports what impl block `index` leaves out, with a fix writing it in.
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

    pub(super) fn impl_target_span(&self, index: usize) -> Span {
        let block = impl_block(self.ast, index);
        let target = block.target.span();
        // With its arguments: `Vector<T, 1>`.
        let Some(&last) = block.target_args.last() else {
            return target;
        };
        let last = self.ast.types.span(last);
        let end = last.start_offset + last.length;
        let text = &target.source.content()[end..];
        let close = end + text.find('>').map_or(0, |i| i + 1);
        Span::from_range(target.start_offset..close, target.source.clone())
    }

    fn check_impl_method(&mut self, index: usize, wanted: Id<Func>, written: Id<Func>) {
        let outer = self.current_func.replace(written);
        let (sig, type_args) = self.instantiate(wanted);
        let mut own_target = None;
        if let (Some(&this), Some(target)) = (type_args.first(), self.impls[index].target) {
            let target = self.in_method_terms(index, target);
            let _ = self.u.unify(this, target);
            own_target = Some(target);
        }
        // The trait's own parameters come after `Self`: what the implementation says they are.
        let trait_args = self.impls[index].trait_args.clone();
        for (&param, &arg) in type_args.iter().skip(1).zip(&trait_args) {
            let arg = self.in_method_terms(index, arg);
            let _ = self.u.unify(param, arg);
        }
        let normalize = |c: &mut Self, ty: TypeId| {
            let ty = c.normalize_assoc(ty);
            match (own_target, c.impls[index].blanket.is_some()) {
                (Some(target), true) => c.own_assoc(index, target, ty),
                _ => ty,
            }
        };
        let params: Vec<TypeId> = sig.params.iter().map(|&p| normalize(self, p)).collect();
        let ret = normalize(self, sig.ret);
        let actual = self.sigs[written.id].clone();
        let fits = params.len() == actual.params.len()
            && (params.iter().zip(&actual.params)).all(|(&p, &a)| self.u.unify(p, a).is_ok())
            && self.u.unify(ret, actual.ret).is_ok();
        if !fits {
            let shown: Vec<String> = (params.iter())
                .map(|&p| self.u.display(p, self.names))
                .collect();
            let name = self.ast.funcs[written].name.clone();
            let ret = match self.u.shallow_ty(ret) {
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

    /// `ty`, in terms of impl block `index`'s parameters, in terms of the checked method's own
    /// copies of them, if it's one of that block's.
    fn in_method_terms(&mut self, index: usize, ty: TypeId) -> TypeId {
        let Some(func) = self.current_func else {
            return ty;
        };
        if self.names.impl_of(func) != Some(index) {
            return ty;
        }
        let params = self.impls[index].params.clone();
        // Checking one instance: what they stand for in it.
        let args: Vec<TypeId> = (self.names.type_params(func).iter().take(params.len()))
            .map(|&p| match self.bound.get(&p) {
                Some(&ty) => ty,
                None => self.u.intern(Ty::Param(p)),
            })
            .collect();
        if args.len() != params.len() {
            return ty;
        }
        let subst: Vec<(DefId, TypeId)> = params.into_iter().zip(args).collect();
        impls::substitute(&mut self.u.table, ty, &subst)
    }

    pub(super) fn self_type(&mut self, index: usize) -> TypeId {
        match self.impls.get(index).and_then(|i| i.target) {
            Some(target) => self.in_method_terms(index, target),
            None => self.u.table.error,
        }
    }

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
    /// implementation: `ListIntoIter<i32>::Item` is `i32`. One of a type parameter, or of a type
    /// not known yet, stays as it is.
    /// `ty` with the associated types of blanket impl `index`'s own trait, of its parameter
    /// `target`, as it says them: `C::Iter` in `impl<C: Index> IterRef for C`'s methods is
    /// its `type Iter`. Nothing else knows them: `C` could be any type.
    fn own_assoc(&mut self, index: usize, target: TypeId, ty: TypeId) -> TypeId {
        let ty = self.u.shallow(ty);
        let t = self.u.get(ty).clone();
        if let Ty::Assoc {
            base,
            trait_,
            index: at,
        } = t
            && Some(trait_) == self.impls[index].trait_
            && self.u.shallow(base) == target
            && let Some(written) = self.impls[index].types.get(at).copied().flatten()
        {
            return self.in_method_terms(index, written);
        }
        if t.children().is_empty() {
            return ty;
        }
        let mapped = t.map_children(|child| self.own_assoc(index, target, child));
        self.u.intern(mapped)
    }

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
        let subst = self.impls[i].subst(&args);
        let found = impls::substitute(&mut self.u.table, written, &subst);
        self.normalize_assoc(found)
    }

    pub(super) fn implements(&self, ty: TypeId, trait_: DefId) -> bool {
        self.implements_given(ty, trait_, &[])
    }

    /// Whether `ty` implements `trait_` given `args`: `Test<i32>`, not just any `Test`. None
    /// given is any.
    pub(super) fn implements_given(&self, ty: TypeId, trait_: DefId, args: &[TypeId]) -> bool {
        let ty = self.u.shallow(ty);
        if let Some(lang) = self.names.lang_of(trait_)
            && self.has_builtin(ty, lang)
        {
            return true;
        }
        // `T?` compares as `T` does, the language seeing to `none`; `&T` as what it refers
        // to, as `==` on references does.
        if self.names.lang_of(trait_) == Some(LangTrait::Eq)
            && let Ty::Optional(inner) | Ty::Ref { to: inner, .. } = *self.u.get(ty)
        {
            return self.implements(inner, trait_);
        }
        // Whatever's copied is `Copy`; a type parameter only if it's required to be.
        if self.names.lang_of(trait_) == Some(LangTrait::Copy)
            && !matches!(self.u.get(ty), Ty::Param(_))
        {
            return self.is_copy(ty);
        }
        match self.u.get(ty) {
            Ty::Error => true,
            Ty::Param(def) => self.bound_has(*def, trait_),
            _ => {
                let u = &self.u;
                find_impl(&self.impls, &u.table, trait_, ty, args, &|t| u.shallow(t)).is_some()
            }
        }
    }

    /// Whether type parameter `param`'s bounds give it `trait_`: one of them is it, or has it
    /// as a supertrait, as `IndexMut` has `Index`.
    pub(super) fn bound_has(&self, param: DefId, trait_: DefId) -> bool {
        let mut traits = self.names.bounds(param).to_vec();
        let mut seen = Vec::new();
        while let Some(t) = traits.pop() {
            if t == trait_ {
                return true;
            }
            let DefKind::Trait(id) = self.names.def(t).kind else {
                continue;
            };
            if !seen.contains(&t) {
                seen.push(t);
                traits.extend(self.names.supers(id));
            }
        }
        false
    }

    /// `func`'s type arguments `args` have to meet their bounds: checked once they're known.
    pub(super) fn require_bounds(&mut self, func: Id<Func>, args: &[TypeId], span: Span) {
        let names = self.names;
        let params = names.type_params(func);
        let subst: Vec<(DefId, TypeId)> = params.iter().copied().zip(args.iter().copied()).collect();
        for (&param, &arg) in params.iter().zip(args) {
            for &trait_ in names.bounds(param) {
                let given = self.bound_args.get(&(param, trait_)).cloned().unwrap_or_default();
                let given = given.into_iter().map(|ty| self.substitute(ty, &subst)).collect();
                self.deferred.obligations.push((arg, trait_, given, span.clone()));
            }
        }
    }

    /// Lowers what every bound gives its trait, `[&I::Item, bool]` for `P: Call<&I::Item,
    /// bool>`, with the trait's defaults for what it leaves out.
    pub(super) fn lower_bound_args(&mut self) {
        let names = self.names;
        for (param, trait_) in names.bounds_given() {
            let DefKind::Trait(t) = names.def(trait_).kind else {
                continue;
            };
            let written = names.bound_args(param, trait_);
            let (Some(&first), Some(&last)) = (written.first(), written.last()) else {
                continue;
            };
            let span = self.ast.types.span(first).merge(self.ast.types.span(last));
            let given = written.iter().map(|&ty| self.lower(ty)).collect();
            let this = self.u.intern(Ty::Param(param));
            let args = self.trait_args(trait_, t, this, given, span);
            self.bound_args.insert((param, trait_), args);
        }
    }

    pub(super) fn check_obligations(&mut self) {
        for (ty, trait_, given, span) in std::mem::take(&mut self.deferred.obligations) {
            let ty = self.normalize_assoc(ty);
            let Some(ty) = self.as_decided(ty) else {
                continue;
            };
            // With one still unknown, any implementation does.
            let mut args = Vec::new();
            for arg in given {
                let arg = self.normalize_assoc(arg);
                match self.as_decided(arg) {
                    Some(arg) => args.push(arg),
                    None => {
                        args.clear();
                        break;
                    }
                }
            }
            if self.implements_given(ty, trait_, &args) {
                continue;
            }
            let mut trait_name = self.names.def(trait_).name.to_string();
            if !args.is_empty() {
                let args: Vec<String> = (args.iter()).map(|&a| self.u.display(a, self.names)).collect();
                trait_name = format!("{trait_name}<{}>", args.join(", "));
            }
            self.errors.push(TypeError::DoesNotImplement {
                ty: self.describe(ty),
                trait_: trait_name,
                span,
            });
        }
    }

    /// `ty` as what nothing else decides makes it: a number an `i32` or `f64`, text a
    /// `string`, `[1, 2, 3]` an array, in an optional or behind a reference too. `None` while
    /// it could be anything.
    fn as_decided(&mut self, ty: TypeId) -> Option<TypeId> {
        match self.u.var_kind(ty) {
            Some(VarKind::Int) => return Some(self.u.table.i32),
            Some(VarKind::Float) => return Some(self.u.table.f64),
            Some(VarKind::Text) => return Some(self.u.table.text),
            Some(VarKind::Collection { element, len }) => {
                return Some(self.u.intern(Ty::Array { element, len }));
            }
            Some(_) => return None,
            None => {}
        }
        match *self.u.shallow_ty(ty) {
            Ty::Optional(inner) => {
                let inner = self.as_decided(inner)?;
                Some(self.u.intern(Ty::Optional(inner)))
            }
            Ty::Ref { mutable, to } => {
                let to = self.as_decided(to)?;
                Some(self.u.intern(Ty::Ref { mutable, to }))
            }
            _ => Some(ty),
        }
    }

    pub(super) fn trait_method(&self, trait_: DefId, name: &str) -> Option<Id<Func>> {
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            return None;
        };
        (self.ast.traits[t].methods.iter().copied())
            .find(|&m| !self.ast.funcs[m].prop && &*self.ast.funcs[m].name.name == name)
    }

    fn assoc_index(&self, trait_: DefId, name: &str) -> Option<usize> {
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            return None;
        };
        (self.names.trait_types(t).iter()).position(|&d| &*self.names.def(d).name == name)
    }

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
                    // Every number type is shown as text: `42.to_string()`.
                    LangTrait::Display,
                ]
                .into_iter()
                .filter_map(|l| self.names.lang(l))
                .collect(),
                _ => return None,
            },
            _ => {
                let mut traits = implemented(self);
                traits.extend(
                    (LangTrait::ALL.iter().copied())
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
        self.records.methods.insert(call, self.names.func(func));
        self.type_args.insert(call, type_args.clone());
        self.require_bounds(func, &type_args, self.span(call));
        let name = format!("{}::{}", self.u.display(base, self.names), method.name);
        match params.split_first() {
            Some((&self_ty, rest)) => {
                if !self.receive(receiver, self_ty, receiver_ty) {
                    self.mismatch(self_ty, receiver_ty, self.span(receiver), None);
                }
                self.check_args_from(call, &name, func, 1, rest, args);
                if let Some(trait_) = traits
                    .iter()
                    .copied()
                    .find(|&t| self.trait_method(t, &method.name) == Some(func))
                {
                    self.settle_trait_args(trait_, base, &type_args);
                }
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

    pub(super) fn iterator_loop(&mut self, stmt: Id<som_ast::Stmt>, ty: TypeId) -> Option<TypeId> {
        let iterator = self.names.lang(LangTrait::Iterator)?;
        let item = self.assoc_index(iterator, LangTrait::Iterator.assoc_type()?)?;
        let shallow = self.u.shallow(ty);
        let (viewed, lent) = match *self.u.get(shallow) {
            Ty::Ref { to, mutable } => (self.u.shallow(to), Some(mutable)),
            _ => (shallow, None),
        };
        // An iterator itself, by value: `for x in list.iter()`. Otherwise what makes one: a
        // value given up makes one with `into_iter`, `&value` with `iter`, `&mut value` with
        // `iter_mut`.
        let (iter_ty, start) = if lent.is_none() && self.implements(shallow, iterator) {
            (shallow, Start::Itself)
        } else {
            let (lang, start) = match lent {
                None => (LangTrait::IterOwned, Start::IntoIter),
                Some(false) => (LangTrait::IterRef, Start::Iter),
                Some(true) => (LangTrait::IterMut, Start::IterMut),
            };
            let trait_ = self.names.lang(lang)?;
            if !self.implements(viewed, trait_) {
                return None;
            }
            let index = self.assoc_index(trait_, lang.assoc_type()?)?;
            let assoc = self.u.intern(Ty::Assoc {
                base: viewed,
                trait_,
                index,
            });
            (self.normalize_assoc(assoc), start)
        };
        let assoc = self.u.intern(Ty::Assoc {
            base: iter_ty,
            trait_: iterator,
            index: item,
        });
        let mut element = self.normalize_assoc(assoc);
        // Lent values that copy are copied: `for n in &numbers` gives numbers. Text nothing
        // has decided yet is the `string` it would end up as, which doesn't copy: asked now,
        // it would look like it could.
        let mut copied = false;
        if start == Start::Iter
            && let Ty::Ref { mutable: false, to } = *self.u.shallow_ty(element)
        {
            self.text_as_string(to);
        }
        if start == Start::Iter
            && let Ty::Ref { mutable: false, to } = *self.u.shallow_ty(element)
            && self.is_copy(to)
        {
            element = to;
            copied = true;
        }
        self.records
            .loops
            .insert(stmt, crate::Loop::Iterator { start, copied });
        Some(element)
    }

    /// Whether `def` is the standard library's, which a program can't change.
    pub(super) fn in_std(&self, def: DefId) -> bool {
        som_lang::is_std_module(&self.ast.modules[self.names.def(def).module].name)
    }

    /// Whether struct `def` implements `Copy`, for any type arguments.
    pub(super) fn implements_copy(&self, def: DefId) -> bool {
        let Some(copy) = self.names.lang(LangTrait::Copy) else {
            return false;
        };
        (self.impls.iter()).any(|info| {
            info.trait_ == Some(copy)
                && info
                    .target
                    .is_some_and(|t| matches!(self.u.get(t), Ty::Struct(d, _) if *d == def))
        })
    }

    pub(super) fn has_builtin(&self, ty: TypeId, lang: LangTrait) -> bool {
        impls::builtin_operator(self.u.shallow_ty(ty), lang)
    }
}

/// Every impl block, in every module in order, as `Names::impls` counts them.
pub(super) fn impl_blocks(ast: &som_ast::Ast) -> impl Iterator<Item = &som_ast::Impl> {
    (ast.modules.iter())
        .flat_map(|m| &m.items)
        .filter_map(|item| match item {
            Item::Impl(block) => Some(block),
            _ => None,
        })
}

fn impl_block(ast: &som_ast::Ast, index: usize) -> &som_ast::Impl {
    impl_blocks(ast)
        .nth(index)
        .expect("an impl block for each index")
}
