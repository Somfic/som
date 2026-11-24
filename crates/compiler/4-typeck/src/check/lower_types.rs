use som_ast::{BinOp, Expr, Func, Id, Primitive, Span, TypeExpr};
use som_base::message;
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::Signature;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId, Value};
use crate::unify::VarKind;

impl Checker<'_> {
    pub(super) fn lower(&mut self, id: Id<TypeExpr>) -> TypeId {
        let ty = match &self.ast.types[id] {
            TypeExpr::Unit => Ty::Unit,
            // Old spelling of `string`: reported, then read as one.
            TypeExpr::Primitive(Primitive::Str) => {
                self.errors.push(TypeError::StrIsString {
                    span: self.ast.types.span(id).clone(),
                });
                return self.u.table.text;
            }
            TypeExpr::Primitive(p) => Ty::from_primitive(*p),
            TypeExpr::Pointer => Ty::Pointer,
            TypeExpr::Dyn { path, args } => self.lower_dyn(id, path, args),
            TypeExpr::PointerTo(to) => Ty::Ptr(self.lower(*to)),
            TypeExpr::Named { args, .. } => match self
                .names
                .ty(id)
                .map(|def| (def, self.names.def(def).kind.clone()))
            {
                Some((def, DefKind::Struct(s))) => {
                    let span = self.ast.types.span(id).clone();
                    Ty::Struct(def, self.lower_args(def, s, args, span))
                }
                Some((def, _))
                    if self.names.is_value_param(def)
                        || matches!(self.names.def(def).kind, DefKind::Const(_)) =>
                {
                    self.errors.push(TypeError::ValueAsType {
                        what: message::var(&*self.names.def(def).name).into(),
                        span: self.ast.types.span(id).clone(),
                    });
                    Ty::Error
                }
                // Checking one instance: what it stands for there.
                Some((def, DefKind::TypeParam { .. } | DefKind::ImplParam { .. }))
                    if let Some(&ty) = self.bound.get(&def) =>
                {
                    return ty;
                }
                Some((
                    def,
                    DefKind::TypeParam { .. }
                    | DefKind::StructParam { .. }
                    | DefKind::TraitParam { .. },
                )) => Ty::Param(def),
                Some((def, DefKind::ImplParam { .. })) => Ty::Param(def),
                Some((def, DefKind::AliasParam { .. })) => match self.alias_args.get(&def) {
                    Some(&ty) => return ty,
                    None => Ty::Param(def),
                },
                Some((_, DefKind::SelfType { impl_ })) => return self.self_type(impl_),
                Some((_, DefKind::ImplType { impl_, index })) => {
                    return self.impl_type(impl_, index);
                }
                Some((_, DefKind::AssocType { trait_, index })) => {
                    let base = self.names.projection_base(id);
                    return self.assoc_type(base, trait_, index);
                }
                Some((def, DefKind::Alias { ty })) => return self.lower_alias(id, def, ty, args),
                _ => Ty::Error, // already reported by the resolver
            },
            // `&str` from before strings were slices.
            TypeExpr::Reference { to, .. }
                if matches!(self.ast.types[*to], TypeExpr::Primitive(Primitive::Str)) =>
            {
                self.errors.push(TypeError::StrIsString {
                    span: self.ast.types.span(id).clone(),
                });
                return self.u.table.text;
            }
            TypeExpr::Reference { mutable, to, .. } => {
                let to = self.lower(*to);
                Ty::Ref {
                    mutable: *mutable,
                    to,
                }
            }
            TypeExpr::Optional(inner) => Ty::Optional(self.lower(*inner)),
            // `T!` is the standard library's `Outcome<T>`. The types it says it fails with are
            // checked to be types; what's done with them is up to the function giving it.
            TypeExpr::Failable { ok, errors } => {
                let ok = self.lower(*ok);
                for &error in errors.iter().flatten() {
                    self.lower(error);
                }
                match self.names.lang_struct(som_lang::LangStruct::Outcome) {
                    Some(outcome) => Ty::Struct(outcome, vec![ok]),
                    None => Ty::Error,
                }
            }
            // `#T` is the standard library's `Rc<T>`; without it loaded, there's nothing it is.
            TypeExpr::Shared(inner) => {
                let inner = self.lower(*inner);
                match self.names.lang_struct(som_lang::LangStruct::Rc) {
                    Some(rc) => Ty::Struct(rc, vec![inner]),
                    None => Ty::Error,
                }
            }
            TypeExpr::Array { element, len } => Ty::Array {
                element: self.lower(*element),
                len: self.lower_value(*len, None),
            },
            TypeExpr::Int(_) | TypeExpr::Bool(_) | TypeExpr::Binary { .. } => {
                self.errors.push(TypeError::ValueAsType {
                    what: message::code(self.ast.type_text(id)).into(),
                    span: self.ast.types.span(id).clone(),
                });
                Ty::Error
            }
            TypeExpr::Elements(element) => Ty::Slice(self.lower(*element)),
            TypeExpr::Slice { element, mutable } => {
                let element = self.lower(*element);
                let slice = self.u.intern(Ty::Slice(element));
                Ty::Ref {
                    mutable: *mutable,
                    to: slice,
                }
            }
            TypeExpr::Fn { params, returns } => {
                let params = params.iter().map(|&p| self.lower(p)).collect();
                let ret = match returns {
                    Some(r) => self.lower(*r),
                    None => self.u.table.unit,
                };
                Ty::Fn { params, ret }
            }
        };
        self.u.intern(ty)
    }

    fn lower_alias(
        &mut self,
        id: Id<TypeExpr>,
        def: DefId,
        ty: Id<TypeExpr>,
        args: &[Id<TypeExpr>],
    ) -> TypeId {
        let params = self.names.alias_params(def).to_vec();
        let span = self.ast.types.span(id).clone();
        // `Vector3` in a `let` annotation: what its arguments are is worked out from the value.
        let args: Vec<TypeId> =
            if args.is_empty() && !params.is_empty() && self.hints.omitted_args_ok {
                (0..params.len())
                    .map(|_| self.u.fresh(VarKind::Any))
                    .collect()
            } else if args.len() != params.len() {
                args.iter().for_each(|&a| {
                    self.lower_arg(a, None);
                });
                self.errors.push(TypeError::WrongTypeArgCount {
                    name: self.names.def(def).name.to_string(),
                    expected: params.len(),
                    found: args.len(),
                    span,
                });
                return self.u.table.error;
            } else {
                (args.iter().zip(&params))
                    .map(|(&arg, &param)| self.lower_arg(arg, Some(param)))
                    .collect()
            };

        if self.expanding.contains(&def) {
            let start = self.expanding.iter().position(|&d| d == def).unwrap_or(0);
            // Report each alias in the cycle once, at its own definition.
            for &alias in &self.expanding[start..] {
                if self.cyclic.insert(alias) {
                    let alias = self.names.def(alias);
                    self.errors.push(TypeError::AliasCycle {
                        name: alias.name.to_string(),
                        span: alias.span.clone(),
                    });
                }
            }
            return self.u.table.error;
        }
        if self.cyclic.contains(&def) {
            return self.u.table.error;
        }
        let lowered = self.expand_alias(def, ty, &params, &args);
        if self.cyclic.contains(&def) {
            return self.u.table.error;
        }
        lowered
    }

    fn expand_alias(
        &mut self,
        def: DefId,
        ty: Id<TypeExpr>,
        params: &[DefId],
        args: &[TypeId],
    ) -> TypeId {
        let outer: Vec<_> = (params.iter().zip(args))
            .map(|(&param, &arg)| (param, self.alias_args.insert(param, arg)))
            .collect();
        // What it stands for is written out in full; nothing in it can be left out.
        let omitted = std::mem::take(&mut self.hints.omitted_args_ok);
        self.expanding.push(def);
        let lowered = self.lower(ty);
        self.expanding.pop();
        self.hints.omitted_args_ok = omitted;
        for (param, outer) in outer {
            match outer {
                Some(ty) => self.alias_args.insert(param, ty),
                None => self.alias_args.remove(&param),
            };
        }
        lowered
    }

    /// An alias naming a struct in an expression, `Vector3::zero()`: its parameters start
    /// unknown, for the value to decide.
    pub(super) fn alias_value(&mut self, def: DefId) -> TypeId {
        let DefKind::Alias { ty } = self.names.def(def).kind else {
            return self.u.table.error;
        };
        if self.cyclic.contains(&def) {
            return self.u.table.error;
        }
        let params = self.names.alias_params(def).to_vec();
        let args: Vec<TypeId> = params.iter().map(|_| self.u.fresh(VarKind::Any)).collect();
        self.expand_alias(def, ty, &params, &args)
    }

    /// The type `func`'s impl block is for; the block's parameters are the first of `type_args`.
    pub(super) fn owner_type(&mut self, func: Id<Func>, type_args: &[TypeId]) -> Option<TypeId> {
        let index = self.names.impl_of(func)?;
        let info = self.impls.get(index)?;
        let target = info.target?;
        let args = type_args.get(..info.params.len())?;
        let subst = info.subst(args);
        Some(crate::impls::substitute(&mut self.u.table, target, &subst))
    }

    /// `Alloc<i32>::with_capacity(10)`: the type `func` is a function of, with the arguments
    /// written for it, has to be the type its impl block is for.
    pub(super) fn fit_owner_args(
        &mut self,
        id: Id<Expr>,
        callee: &som_ast::Path,
        func: Id<Func>,
        type_args: &[TypeId],
        written: &[Id<TypeExpr>],
    ) {
        let def = self.names.func(func);
        if let Some(ty) = self.owner_written(id, callee, def, written)
            && let Some(target) = self.owner_type(func, type_args)
            && self.u.unify(target, ty).is_err()
        {
            self.mismatch(target, ty, self.span(id), None);
        }
    }

    /// `Alloc<i32>` in `Alloc<i32>::with_capacity(10)`. `None` if none is written, or, with an
    /// error, if `def` isn't a struct's function.
    pub(super) fn owner_written(
        &mut self,
        id: Id<Expr>,
        callee: &som_ast::Path,
        def: DefId,
        written: &[Id<TypeExpr>],
    ) -> Option<TypeId> {
        if written.is_empty() {
            return None;
        }
        let DefKind::Func(func) = self.names.def(def).kind else {
            return None;
        };
        let owner = (self.names.impl_of(func))
            .and_then(|i| self.names.impls[i].owner)
            .filter(|&def| matches!(self.names.def(def).kind, DefKind::Struct(_)));
        let Some(owner) = owner else {
            written.iter().for_each(|&t| {
                self.lower_arg(t, None);
            });
            let path = &callee.segments[..callee.segments.len() - 1];
            self.errors.push(TypeError::WrongTypeArgCount {
                name: path.iter().map(|s| &*s.name).collect::<Vec<_>>().join("::"),
                expected: 0,
                found: written.len(),
                span: self.span(id),
            });
            return None;
        };
        let DefKind::Struct(s) = self.names.def(owner).kind else {
            unreachable!("filtered to structs")
        };
        let args = self.lower_args(owner, s, written, self.span(id));
        Some(self.u.intern(Ty::Struct(owner, args)))
    }

    /// `ty`, of expression `id`, has to be the type of the alias its path names a struct
    /// through, if it does: `Vector3::zero()` is a `Vector<3, f32>`.
    pub(super) fn fit_alias(&mut self, id: Id<Expr>, ty: TypeId) {
        let Some(alias) = self.names.expr_alias(id) else {
            return;
        };
        let aliased = self.alias_value(alias);
        if self.u.unify(aliased, ty).is_err() {
            self.mismatch(aliased, ty, self.span(id), None);
        }
    }

    pub(super) fn lower_args(
        &mut self,
        def: DefId,
        s: som_ast::Id<som_ast::Struct>,
        args: &[Id<TypeExpr>],
        span: Span,
    ) -> Vec<TypeId> {
        let params = self.names.struct_params(s);
        // `List` in a `let` annotation: what they are is worked out from the value.
        if args.is_empty() && !params.is_empty() && self.hints.omitted_args_ok {
            return (0..params.len())
                .map(|_| self.u.fresh(VarKind::Any))
                .collect();
        }
        let params = params.to_vec();
        let defaults: Vec<Option<Id<TypeExpr>>> = (self.ast.structs[s].generics.iter())
            .map(|g| g.default)
            .collect();
        let required = defaults.iter().take_while(|d| d.is_none()).count();
        if args.len() < required || args.len() > params.len() {
            args.iter().for_each(|&a| {
                self.lower_arg(a, None);
            });
            self.wrong_arg_count(def, required, params.len(), args.len(), span);
            return vec![self.u.table.error; params.len()];
        }
        let given: Vec<TypeId> = (args.iter().zip(&params))
            .map(|(&arg, &param)| self.lower_arg(arg, Some(param)))
            .collect();
        self.with_defaults(&params, &defaults, given)
    }

    /// Defaults can name the parameters before them: `B` in `struct Pair<A, B = A>` is `A`'s
    /// argument if it's left out.
    pub(super) fn with_defaults(
        &mut self,
        params: &[DefId],
        defaults: &[Option<Id<TypeExpr>>],
        mut given: Vec<TypeId>,
    ) -> Vec<TypeId> {
        for index in given.len()..params.len() {
            let Some(default) = defaults.get(index).copied().flatten() else {
                given.push(self.u.table.error);
                continue;
            };
            let ty = self.lower(default);
            let subst: Vec<(DefId, TypeId)> =
                params.iter().copied().zip(given.iter().copied()).collect();
            let ty = self.substitute(ty, &subst);
            given.push(ty);
        }
        given
    }

    pub(super) fn wrong_arg_count(
        &mut self,
        def: DefId,
        required: usize,
        most: usize,
        found: usize,
        span: Span,
    ) {
        let name = self.names.def(def).name.to_string();
        self.errors.push(match required == most {
            true => TypeError::WrongTypeArgCount {
                name,
                expected: most,
                found,
                span,
            },
            false => TypeError::WrongTypeArgRange {
                name,
                required,
                most,
                found,
                span,
            },
        });
    }

    fn lower_arg(&mut self, arg: Id<TypeExpr>, param: Option<DefId>) -> TypeId {
        match param.and_then(|p| self.value_params.get(&p).copied()) {
            Some(ty) => self.lower_value(arg, Some(ty)),
            None => self.lower(arg),
        }
    }

    /// A value in a type argument's place; with no `expected`, an array length of any integer
    /// type.
    pub(super) fn lower_value(&mut self, id: Id<TypeExpr>, expected: Option<TypeId>) -> TypeId {
        let span = self.ast.types.span(id).clone();
        let error = self.u.table.error;
        let (value, found) = match self.ast.types[id] {
            TypeExpr::Int(n) => return self.lower_number(n, expected, span),
            TypeExpr::Named { ref args, .. }
                if args.is_empty()
                    && let Some(def) = self.names.ty(id)
                    && let DefKind::Const(c) = self.names.def(def).kind =>
            {
                let number = match self.known.get(c) {
                    Some(crate::ConstValue::Int(n)) => Some(n),
                    Some(_) => None,
                    None => self.const_number(self.ast.consts[c].value),
                };
                return match number {
                    Some(n) if n >= 0 => self.lower_number(n as u128, expected, span),
                    // Worked out by running code, not yet: checked again once it is.
                    None if self.need(c) => error,
                    _ => {
                        self.errors.push(TypeError::ConstNotCount {
                            name: self.names.def(def).name.to_string(),
                            span,
                            defined_at: self.names.def(def).span.clone(),
                        });
                        error
                    }
                };
            }
            TypeExpr::Bool(b) => (Ty::Value(Value::Bool(b)), self.u.table.bool),
            TypeExpr::Binary { op, lhs, rhs } => {
                if let Some(ty) = expected
                    && !matches!(self.u.get(ty), Ty::Int(_) | Ty::Error)
                {
                    let found = self.u.fresh(VarKind::Int);
                    self.mismatch(ty, found, span, None);
                    return error;
                }
                let (lhs, rhs) = (
                    self.lower_value(lhs, expected),
                    self.lower_value(rhs, expected),
                );
                if lhs == error || rhs == error {
                    return error;
                }
                let ty = match op {
                    BinOp::Multiply => Ty::Mul(lhs, rhs),
                    _ => Ty::Add(lhs, rhs),
                };
                let ty = self.u.intern(ty);
                return self.u.normalize(ty);
            }
            TypeExpr::Named { ref args, .. }
                if args.is_empty()
                    && let Some(def) = self.names.ty(id)
                    && let Some(&value) = self.alias_args.get(&def) =>
            {
                return value;
            }
            // Checking one instance: the number it stands for there.
            TypeExpr::Named { ref args, .. }
                if args.is_empty()
                    && let Some(def) = self.names.ty(id)
                    && let Some(&ty) = self.bound.get(&def) =>
            {
                return ty;
            }
            TypeExpr::Named { ref args, .. }
                if args.is_empty()
                    && let Some(def) = self.names.ty(id)
                    && let Some(&ty) = self.value_params.get(&def) =>
            {
                (Ty::Param(def), ty)
            }
            // Already reported by the resolver.
            TypeExpr::Named { .. } if self.names.ty(id).is_none() => return error,
            _ => {
                let what = message::code(self.ast.type_text(id)).into();
                let ty = expected.map(|ty| self.describe(ty));
                self.errors.push(TypeError::TypeAsValue { what, ty, span });
                return error;
            }
        };
        // An array's length can be any integer.
        let fits = match expected {
            Some(expected) => self.u.unify(expected, found).is_ok(),
            None => {
                matches!(self.u.get(found), Ty::Int(_) | Ty::Error)
                    || self.u.var_kind(found) == Some(VarKind::Int)
            }
        };
        if !fits {
            let expected = expected.unwrap_or(self.u.table.usize);
            self.mismatch(expected, found, span, None);
            return error;
        }
        self.u.intern(value)
    }

    pub(super) fn fresh_subst(&mut self, params: &[DefId]) -> Vec<(DefId, TypeId)> {
        (params.iter())
            .map(|&p| (p, self.u.fresh(VarKind::Any)))
            .collect()
    }

    pub(super) fn struct_subst(
        &self,
        s: som_ast::Id<som_ast::Struct>,
        args: &[TypeId],
    ) -> Vec<(DefId, TypeId)> {
        let params = self.names.struct_params(s);
        params.iter().copied().zip(args.iter().copied()).collect()
    }

    pub(super) fn instantiate(&mut self, func: Id<Func>) -> (Signature, Vec<TypeId>) {
        // A return type that isn't written, or a property's that leaves out arguments, is what
        // the body decides: check it first.
        if self.ast.funcs[func].returns.is_none() || self.ast.funcs[func].prop {
            self.check_func(func);
        }
        let mut sig = self.sigs[func.id].clone();
        let type_params = self.names.type_params(func);
        if type_params.is_empty() {
            return (sig, Vec::new());
        }
        // Still undecided, called from inside its own checking: the call works its type out on
        // its own rather than deciding the body's for every instantiation.
        if matches!(self.u.shallow_ty(sig.ret), Ty::Var(_)) {
            sig.ret = self.u.fresh(VarKind::Any);
        }
        let args: Vec<TypeId> = type_params
            .iter()
            .map(|_| self.u.fresh(VarKind::Any))
            .collect();
        let subst: Vec<(DefId, TypeId)> = type_params
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect();
        let sig = Signature {
            params: sig
                .params
                .iter()
                .map(|&p| self.substitute(p, &subst))
                .collect(),
            ret: self.substitute(sig.ret, &subst),
        };
        (sig, args)
    }

    /// The generic parameters type arguments written for a call of `func` stand for: its own,
    /// after its impl block's; or if it has none, its impl block's, which are the type's.
    /// `Alloc::new<u8>()` is a `new` of `Alloc<u8>`.
    pub(super) fn own_type_params(&self, func: Id<Func>) -> Vec<DefId> {
        let all = self.names.type_params(func);
        let inherited = (self.names.impl_of(func))
            .map_or(0, |i| self.names.impls[i].params.len())
            .min(all.len());
        if inherited == all.len() {
            return all.to_vec();
        }
        all[inherited..].to_vec()
    }

    /// `null<i32>()`: written type arguments stand for `func`'s own parameters, whose
    /// variables are the last of `type_args`.
    pub(super) fn written_type_args(
        &mut self,
        call: Id<Expr>,
        func: Id<Func>,
        written: &[Id<TypeExpr>],
        type_args: &[TypeId],
    ) {
        let own = self.own_type_params(func);
        if written.len() != own.len() {
            written.iter().for_each(|&a| {
                self.lower_arg(a, None);
            });
            self.errors.push(TypeError::WrongTypeArgCount {
                name: self.ast.funcs[func].name.name.to_string(),
                expected: own.len(),
                found: written.len(),
                span: self.span(call),
            });
            return;
        }
        let inherited = type_args.len() - own.len();
        for ((&arg, &param), &var) in written.iter().zip(&own).zip(&type_args[inherited..]) {
            let ty = self.lower_arg(arg, Some(param));
            let _ = self.u.unify(var, ty);
        }
    }

    pub(super) fn substitute(&mut self, ty: TypeId, subst: &[(DefId, TypeId)]) -> TypeId {
        // An unwritten return type is a variable its body decided: what it was decided to be.
        let ty = self.u.shallow(ty);
        match self.u.get(ty).clone() {
            Ty::Param(def) => crate::impls::substituted(subst, def).unwrap_or(ty),
            other if other.children().is_empty() => ty,
            other => {
                let substituted = other.map_children(|child| self.substitute(child, subst));
                let substituted = self.u.intern(substituted);
                // `N + 1` with `N` filled in may add up: `3 + 1` is `4`; and `I::Item` with
                // `I` filled in may be known.
                let substituted = self.u.normalize(substituted);
                self.normalize_assoc(substituted)
            }
        }
    }

    /// Reports arithmetic in types that turned out unequal once more was known, like a
    /// `[T; R * C]` that's `[T; 6]` with `R` and `C` both 2. Where's no longer known, so it's
    /// reported at `span`, the function it was found in.
    pub(super) fn settle(&mut self, span: Span) {
        for (expected, found) in self.u.settle() {
            self.mismatch(expected, found, span.clone(), None);
        }
    }
}
