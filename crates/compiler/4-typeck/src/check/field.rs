use som_ast::{Expr, Func, Id};
use som_lang::LangTrait;
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::ArrayMethod;
use crate::error::TypeError;
use crate::traits::find_impl;
use crate::ty::{Ty, TypeId, Value};
use crate::unify::VarKind;

impl Checker<'_> {
    pub(super) fn infer_field(
        &mut self,
        id: Id<Expr>,
        object: Id<Expr>,
        field: &som_ast::Ident,
    ) -> TypeId {
        let error = self.u.table.error;
        let object_ty = self.infer_object(object);
        // `node.value` of a `#Node`: what it shares.
        let object_ty = self.through_shared(object, object_ty, &field.name);
        // `"abc".len`: text in quotes is a `string`, with its props.
        self.text_as_string(object_ty);
        // `list.len`, with a private field and a `pub prop` both called `len`: the
        // field where it's seen, so its own code can change it, the prop elsewhere.
        if !self.field_seen(object_ty, field)
            && let Some(ty) = self.prop_access(id, object, object_ty, field)
        {
            return ty;
        }
        // `items.len`: the length of an array, slice or text.
        if ArrayMethod::from_name(&field.name) == Some(ArrayMethod::Len)
            && let Some(ty) = self.array_method(id, object, object_ty, field, &[], true)
        {
            return ty;
        }
        // `xs.length`: an array or slice has `len` and its methods, nothing else.
        if self.is_collection(object_ty) {
            let methods = ArrayMethod::method_names(false);
            self.errors.push(TypeError::UnknownField {
                ty: self.u.display(self.u.shallow(object_ty), self.names),
                field: field.name.to_string(),
                available: Vec::new(),
                props: vec![ArrayMethod::Len.name().into()],
                methods,
                span: field.span.clone(),
            });
            return error;
        }
        // A range's two ends.
        let mut viewed = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = *self.u.get(viewed) {
            viewed = self.u.shallow(to);
        }
        // `items.len` of something with a method of that name, not a field.
        if matches!(
            self.u.get(viewed),
            Ty::Array { .. } | Ty::Slice(_) | Ty::Str
        ) && ArrayMethod::from_name(&field.name).is_some()
        {
            self.errors.push(TypeError::FieldIsMethod {
                ty: self.describe(object_ty),
                name: field.name.to_string(),
                span: field.span.clone(),
            });
            return error;
        }
        if let Ty::Range(int) = *self.u.get(viewed) {
            let index = match &*field.name {
                som_lang::RANGE_START => 0,
                som_lang::RANGE_END => 1,
                _ => {
                    self.errors.push(TypeError::UnknownField {
                        ty: self.u.display(viewed, self.names),
                        field: field.name.to_string(),
                        available: vec![som_lang::RANGE_START.into(), som_lang::RANGE_END.into()],
                        props: Vec::new(),
                        methods: Vec::new(),
                        span: field.span.clone(),
                    });
                    return error;
                }
            };
            self.records.fields.insert(id, index);
            return int;
        }
        let Some((owner, args)) = self.struct_of(object_ty, object) else {
            return error;
        };
        let DefKind::Struct(s) = self.names.def(owner).kind else {
            unreachable!("struct_of returns structs")
        };
        if self.ast.structs[s].is_enum() {
            return self.common_field(id, s, &args, field);
        }
        let fields = &self.ast.structs[s].fields;
        match fields.iter().position(|f| f.name.name == field.name) {
            Some(index) => {
                self.records.fields.insert(id, index);
                self.check_field_visible(s, index, &field.span);
                // Written in terms of the struct's type parameters: fill in its arguments.
                let subst = self.struct_subst(s, &args);
                self.substitute(self.records.structs[s.id][index], &subst)
            }
            None if self.names.method(owner, &field.name).is_some() => {
                self.errors.push(TypeError::FieldIsMethod {
                    ty: self.describe(object_ty),
                    name: field.name.to_string(),
                    span: field.span.clone(),
                });
                error
            }
            None => {
                let (available, props, methods) = self.members(owner, object_ty, &field.span);
                self.errors.push(TypeError::UnknownField {
                    ty: self.names.def(owner).name.to_string(),
                    field: field.name.to_string(),
                    available,
                    props,
                    methods,
                    span: field.span.clone(),
                });
                error
            }
        }
    }

    pub(super) fn struct_of(&mut self, ty: TypeId, expr: Id<Expr>) -> Option<(DefId, Vec<TypeId>)> {
        let outer = self.u.shallow(ty);
        let mut ty = outer;
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
        }
        // `&mut x` of an `x` that's already a reference: say so, not that it isn't a struct.
        if let Ty::Ref { to, .. } = *self.u.get(ty)
            && matches!(self.u.shallow_ty(to), Ty::Struct(..))
        {
            let span = self.span(expr);
            self.errors.push(TypeError::ReferenceToReference {
                object: span.get_text().to_string(),
                ty: self.describe(outer),
                span,
            });
            return None;
        }
        match self.u.get(ty) {
            Ty::Struct(def, args) => Some((*def, args.clone())),
            Ty::Error => None,
            Ty::Var(_) if self.u.var_kind(ty) == Some(VarKind::Any) => {
                self.errors.push(TypeError::CannotInfer {
                    span: self.span(expr),
                });
                None
            }
            // `get(i).value`: it may hold nothing, which is what to say.
            Ty::Optional(_) => {
                let span = self.span(expr);
                self.errors.push(TypeError::MemberOfOptional {
                    object: span.get_text().to_string(),
                    ty: self.describe(ty),
                    variable: matches!(self.ast.exprs[expr], Expr::Var(_)),
                    span,
                });
                None
            }
            _ => {
                self.errors.push(TypeError::NotAStruct {
                    ty: self.describe(ty),
                    span: self.span(expr),
                });
                None
            }
        }
    }

    /// `values[0]`: the element of an array at an index it's known to have, a number written
    /// out that's less than its length. Anything that might be out of bounds is `get`'s.
    /// `values[i]`. An array's element, where `i` is known to be inside it: a number written
    /// out, or a variable below its length (`for i in 0..values.len`, `if i < values.len`). A
    /// slice's length is only known as the program runs, so a slice gives `&T?`: the element,
    /// or `none` past its end. Nothing is checked as it runs: what can't be known isn't allowed.
    pub(super) fn index(&mut self, id: Id<Expr>, object: Id<Expr>, index: Id<Expr>) -> TypeId {
        let error = self.u.table.error;
        let object_ty = self.infer_object(object);
        // `xs[i]` of what implements `Index`: a `List`, `string`, a slice. What its `at`
        // lends, as a place; `at_mut`'s where it's changed (see `index_mut`). That `i` is
        // below its `len` is the prover's to check.
        let mut base = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = *self.u.get(base) {
            base = self.u.shallow(to);
        }
        let is_slice = matches!(self.u.get(base), Ty::Slice(_));
        if is_slice || matches!(self.u.get(base), Ty::Struct(..)) {
            if let Some(ty) = self.index_call(id, object, object_ty, base, index, LangTrait::Index)
            {
                return ty;
            }
            self.errors.push(TypeError::IndexNotArray {
                ty: self.describe(object_ty),
                span: self.span(object),
                get: self.get_suggestion(object, index),
            });
            self.infer(index);
            return error;
        }
        // A number written out is a position; a variable can be any integer type.
        let written = matches!(self.ast.exprs[index], Expr::Int(_));
        let index_ty = if written {
            self.u.table.usize
        } else {
            self.u.fresh(VarKind::Int)
        };
        self.expect(index, index_ty, None);
        // `[1, 2, 3][0]`: a collection nothing else decides is an array.
        if let Some(VarKind::Collection { element, len }) = self.u.var_kind(object_ty) {
            let array = self.u.intern(Ty::Array { element, len });
            let _ = self.u.unify(object_ty, array);
        }
        // Elements are reachable through one reference, as fields are.
        let mut viewed = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = *self.u.get(viewed) {
            viewed = self.u.shallow(to);
        }
        let (element, len) = match *self.u.get(viewed) {
            Ty::Array { element, len } => (element, len),
            Ty::Error => return error,
            _ => {
                self.errors.push(TypeError::IndexNotArray {
                    ty: self.describe(object_ty),
                    span: self.span(object),
                    get: self.get_suggestion(object, index),
                });
                return error;
            }
        };
        let len_ty = self.u.normalize(len);
        // Anything but a number written out: the prover checks it's below the length.
        let Expr::Int(at) = self.ast.exprs[index] else {
            return element;
        };
        match *self.u.shallow_ty(len_ty) {
            Ty::Value(Value::Int(n)) if (at as u128) < n => {}
            Ty::Value(Value::Int(n)) => self.errors.push(TypeError::IndexOutOfBounds {
                index: at as u128,
                len: n,
                span: self.span(index),
            }),
            // `N`, a value parameter: the prover checks it against what's known of `N` there,
            // `if N >= 3` or `@where N >= 3`.
            Ty::Error | Ty::Param(_) => {}
            _ => self.errors.push(TypeError::IndexUnknownLength {
                len: self.u.display(len_ty, self.names),
                span: self.span(index),
                get: self.get_suggestion(object, index),
            }),
        }
        element
    }

    /// `object`, of type `ty`, read through to what it shares when it's a `#T` without a
    /// member called `name` of its own: `node.value` is `node.get().value`. Gives the type
    /// it's read as: the `&T` its `get` lends, which can't change what it refers to.
    pub(super) fn through_shared(&mut self, object: Id<Expr>, ty: TypeId, name: &str) -> TypeId {
        let Some(rc) = self.names.lang_struct(som_lang::LangStruct::Rc) else {
            return ty;
        };
        let Ty::Struct(def, ref args) = *self.u.shallow_ty(ty) else {
            return ty;
        };
        let [inner] = args[..] else {
            return ty;
        };
        let own = self.names.method(rc, name).is_some() || !self.names.props(rc, name).is_empty();
        if def != rc || own {
            return ty;
        }
        let lent = self.u.intern(Ty::Ref {
            mutable: false,
            to: inner,
        });
        self.records.shared.insert(object);
        lent
    }

    /// `values[i]` of type `base`: a call of `method`, `at` from its `impl Index` or
    /// `at_mut` from its `impl IndexMut`, giving what it lends. `None` if it has none.
    fn index_call(
        &mut self,
        id: Id<Expr>,
        object: Id<Expr>,
        object_ty: TypeId,
        base: TypeId,
        key: Id<Expr>,
        lang: LangTrait,
    ) -> Option<TypeId> {
        let method = lang.method()?;
        let index_trait = self.names.lang(lang)?;
        let (at, _) = {
            let u = &self.u;
            find_impl(&self.impls, &u.table, index_trait, base, &[], &|t| {
                u.shallow(t)
            })?
        };
        let func = (self.impls[at].methods.iter().copied())
            .find(|&m| &*self.ast.funcs[m].name.name == method)?;
        let (sig, type_args) = self.instantiate(func);
        self.records.methods.insert(id, self.names.func(func));
        self.require_bounds(func, &type_args, self.span(id));
        if !type_args.is_empty() {
            self.type_args.insert(id, type_args);
        }
        let (&self_ty, rest) = sig.params.split_first()?;
        if !self.receive(object, self_ty, object_ty) {
            self.mismatch(self_ty, object_ty, self.span(object), None);
        }
        let label = format!("{}::{method}", self.u.display(base, self.names));
        self.check_args_from(id, &label, func, 1, rest, &[key]);
        let ret = self.normalize_assoc(sig.ret);
        Some(match *self.u.shallow_ty(ret) {
            Ty::Ref { to, .. } => to,
            _ => ret,
        })
    }

    /// `xs[i]`, through `Index::at`, used where it's changed: `xs[i] = v`, `&mut xs[i]`,
    /// `xs[i].push(v)`. Becomes a call of `IndexMut::at_mut`, or, for a type without one,
    /// an error. False if `expr` isn't such an index.
    pub(super) fn index_mut(&mut self, expr: Id<Expr>) -> bool {
        let Expr::Index { object, index } = self.ast.exprs[expr] else {
            return false;
        };
        let Some(&def) = self.records.methods.get(&expr) else {
            return false;
        };
        let DefKind::Func(func) = self.names.def(def).kind else {
            return false;
        };
        if Some(&*self.ast.funcs[func].name.name) != LangTrait::Index.method() {
            return true;
        }
        let Some(object_ty) = self.exprs[object.id] else {
            return true;
        };
        let mut base = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = *self.u.get(base) {
            base = self.u.shallow(to);
        }
        let read = self.exprs[expr.id];
        // How `at` took `xs` doesn't count: `at_mut` takes it its own way.
        self.records.receivers.remove(&object);
        let changed = self.index_call(expr, object, object_ty, base, index, LangTrait::IndexMut);
        match (changed, read) {
            (Some(changed), Some(read)) => {
                let _ = self.u.unify(changed, read);
            }
            _ => {
                let (object_text, index_text) =
                    (self.span(object).get_text(), self.span(index).get_text());
                self.errors.push(TypeError::SliceIndexWrite {
                    ty: self.describe(object_ty),
                    span: self.span(expr),
                    instead: format!("{object_text}.get_mut({index_text})"),
                });
            }
        }
        true
    }

    /// Whether `ty`, or what it refers to, is an array or slice, or a collection literal.
    fn is_collection(&self, ty: TypeId) -> bool {
        let mut ty = self.u.shallow(ty);
        if let Ty::Ref { to, .. } = *self.u.get(ty) {
            ty = self.u.shallow(to);
        }
        matches!(self.u.get(ty), Ty::Array { .. } | Ty::Slice(_))
            || matches!(self.u.var_kind(ty), Some(VarKind::Collection { .. }))
    }

    pub(super) fn props_for(&mut self, owner: DefId, ty: TypeId) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for (name, def) in self.names.props_of(owner) {
            let DefKind::Func(func) = self.names.def(def).kind else {
                continue;
            };
            if names.last().is_some_and(|last| last == name) {
                continue;
            }
            let (_, type_args) = self.instantiate(func);
            if self.impl_target_fits(func, &type_args, ty) {
                names.push(name.to_string());
            }
        }
        names
    }

    /// Whether `expr` reads a property, `list.len`, rather than a field. Known once it's
    /// inferred.
    pub(super) fn is_prop(&self, expr: Id<Expr>) -> bool {
        matches!(self.ast.exprs[expr], Expr::Field { .. })
            && self.records.methods.contains_key(&expr)
    }

    /// `list.len` of a property, `prop len: usize`: a call of it, without
    /// parentheses. `None` if `object` has no property `field`; fields are named apart.
    fn prop_access(
        &mut self,
        id: Id<Expr>,
        object: Id<Expr>,
        object_ty: TypeId,
        field: &som_ast::Ident,
    ) -> Option<TypeId> {
        if let Some(ty) = self.bound_prop(id, object, object_ty, field) {
            return Some(ty);
        }
        let owner = self.primitive_owner(object_ty).or_else(|| {
            let mut ty = self.u.shallow(object_ty);
            if let Ty::Ref { to, .. } = self.u.get(ty) {
                ty = self.u.shallow(*to);
            }
            match self.u.get(ty) {
                Ty::Struct(def, _) => Some(*def),
                _ => None,
            }
        })?;
        let candidates = self.names.props(owner, &field.name);
        if candidates.is_empty() {
            return None;
        }
        let func_of = |c: &Self, def: DefId| match c.names.def(def).kind {
            DefKind::Func(func) => func,
            _ => unreachable!("properties are functions"),
        };
        // Text written in quotes is a `string` here.
        if self.u.var_kind(object_ty) == Some(VarKind::Text) {
            let _ = self.u.unify(object_ty, self.u.table.text);
        }
        // Of the impl blocks with one, the one for the value's type: `x` of a `Vector<3, T>`.
        // If several fit a value with numbers nothing has decided, `[1.0, 2.0]`, they're
        // settled to `f64` and `i32` first, as they would be anyway.
        let fitting = |c: &mut Self| {
            let mut fitting = Vec::new();
            for &def in &candidates {
                let func = func_of(c, def);
                let (sig, type_args) = c.instantiate(func);
                // One only some instances have, where it isn't one of them, doesn't fit.
                if c.impl_target_fits(func, &type_args, object_ty)
                    && c.member_holds(func, object_ty) != Some(false)
                {
                    fitting.push((def, func, sig, type_args));
                }
            }
            fitting
        };
        let mut found = fitting(self);
        if found.len() > 1 && self.default_numbers(object_ty) {
            found = fitting(self);
        }
        let chosen = found.into_iter().next();
        let Some((def, func, sig, type_args)) = chosen else {
            let funcs: Vec<Id<Func>> = candidates.iter().map(|&d| func_of(self, d)).collect();
            // It's on the type, but only where an `@if` holds that doesn't here.
            if let Some(&only) = (funcs.iter()).find(|&&f| !self.ast.funcs[f].when.is_empty()) {
                let (_, type_args) = self.instantiate(only);
                if self.impl_target_fits(only, &type_args, object_ty) {
                    self.check_member_there(only, object_ty, &field.name, field.span.clone());
                    return Some(self.u.table.error);
                }
            }
            self.not_on_impl_target(&funcs, object_ty, field, "property");
            return Some(self.u.table.error);
        };
        if !self.check_member_there(func, object_ty, &field.name, field.span.clone()) {
            return Some(self.u.table.error);
        }
        self.records.methods.insert(id, def);
        self.check_visible(def, &field.span);
        self.require_bounds(func, &type_args, self.span(id));
        if !type_args.is_empty() {
            self.type_args.insert(id, type_args);
        }
        // `self` is its only parameter; the resolver reports it if that's not so.
        if let Some(&self_ty) = sig.params.first()
            && !self.receive(object, self_ty, object_ty)
        {
            let from = (self.ast.funcs[func].params[0].ty).map(|t| self.ast.types.span(t).clone());
            self.mismatch(self_ty, object_ty, self.span(object), from);
        }
        Some(sig.ret)
    }

    /// `x.len` of an `x` whose type is a type parameter, or `Self` in a trait: the property
    /// of a trait it has to implement, or one that trait builds on.
    fn bound_prop(
        &mut self,
        id: Id<Expr>,
        object: Id<Expr>,
        object_ty: TypeId,
        field: &som_ast::Ident,
    ) -> Option<TypeId> {
        let mut base = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = *self.u.get(base) {
            base = self.u.shallow(to);
        }
        let Ty::Param(param) = *self.u.get(base) else {
            return None;
        };
        let mut traits = self.names.bounds(param).to_vec();
        let mut i = 0;
        while i < traits.len() {
            if let DefKind::Trait(t) = self.names.def(traits[i]).kind {
                for &s in self.names.supers(t) {
                    if !traits.contains(&s) {
                        traits.push(s);
                    }
                }
            }
            i += 1;
        }
        let func = traits.iter().find_map(|&t| {
            let DefKind::Trait(t) = self.names.def(t).kind else {
                return None;
            };
            (self.ast.traits[t].methods.iter().copied())
                .find(|&m| self.ast.funcs[m].prop && self.ast.funcs[m].name.name == field.name)
        })?;
        let (sig, type_args) = self.instantiate(func);
        let _ = self.u.unify(type_args[0], base);
        self.records.methods.insert(id, self.names.func(func));
        self.type_args.insert(id, type_args);
        if let Some(&self_ty) = sig.params.first()
            && !self.receive(object, self_ty, object_ty)
        {
            self.mismatch(self_ty, object_ty, self.span(object), None);
        }
        Some(self.normalize_assoc(sig.ret))
    }
}
