use som_ast::{Func, Id, Span, Struct};
use som_lang::Builtin;
use som_mir::*;
use som_resolve::DefId;
use som_typeck::traits::{ImplInfo, resolve_assoc};
use std::collections::HashMap;
use std::rc::Rc;

use super::{InstanceErrors, Job, Lowerer, Owner, TypedRef};

impl<'a> Lowerer<'a> {
    pub(super) fn struct_instance(&mut self, s: Id<Struct>, args: Vec<TypeId>) {
        for &arg in &args {
            self.make_structs_in(arg);
        }
        let ast_struct = &self.ast.structs[s];
        let def = self.names.strukt(s);
        let subst = Subst::new(self.names.struct_params(s), &args, &self.typed.impls);
        let mut fields: Vec<(String, TypeId)> = (ast_struct.fields.iter())
            .zip(self.typed.fields(s))
            .map(|(f, &ty)| {
                (
                    f.name.name.to_string(),
                    subst.apply(&mut self.mir.types, ty),
                )
            })
            .collect();
        let variants = (ast_struct.is_enum()).then(|| self.enum_layout(s, &subst, &mut fields));
        let name = self.qualified(def, &ast_struct.name.name);
        let name = self.with_type_args(name, &args);
        self.mir.add_struct(StructDef {
            def,
            args,
            name,
            fields,
            variants,
            copy: ast_struct.copy || self.implements_copy(def),
            handle: ast_struct.handle,
        });
    }

    /// Adds the enum's tag, common fields and variants' own fields, as `StructDef::fields`
    /// lays them out.
    fn enum_layout(
        &mut self,
        s: Id<Struct>,
        subst: &Subst,
        fields: &mut Vec<(String, TypeId)>,
    ) -> Vec<VariantDef> {
        let ast_struct = &self.ast.structs[s];
        let tag = self.mir.types.intern(Ty::Int(IntTy::U32));
        fields.push((ENUM_TAG.into(), tag));
        let payload: Vec<TypeId> = (self.typed.fields(s).iter())
            .map(|&ty| subst.apply(&mut self.mir.types, ty))
            .collect();
        let mut common_at = HashMap::new();
        for common in self.typed.common_fields(s) {
            for &at in &common.payload {
                common_at.insert(at, fields.len());
            }
            fields.push((common.name.to_string(), payload[common.payload[0]]));
        }
        (ast_struct.variants().iter().enumerate())
            .map(|(i, v)| {
                let start = ast_struct.payload_start(i);
                let indexes = (v.fields.iter().enumerate())
                    .map(|(k, f)| match common_at.get(&(start + k)) {
                        Some(&at) => at,
                        None => {
                            let name = format!("{}.{}", v.name, f.name);
                            fields.push((name, payload[start + k]));
                            fields.len() - 1
                        }
                    })
                    .collect();
                VariantDef {
                    name: v.name.name.to_string(),
                    fields: indexes,
                }
            })
            .collect()
    }

    /// `name<i32, bool>`, or just `name` without type arguments.
    fn with_type_args(&self, name: String, args: &[TypeId]) -> String {
        if args.is_empty() {
            return name;
        }
        let shown: Vec<String> = args.iter().map(|&a| self.mir.display_ty(a)).collect();
        format!("{name}<{}>", shown.join(", "))
    }

    /// Prefixed with its module, unless that's the root module.
    pub(super) fn qualified(&self, def: DefId, name: &str) -> String {
        let module = &self.ast.modules[self.names.def(def).module].name;
        if module.is_empty() {
            name.to_string()
        } else {
            format!("{module}::{name}")
        }
    }

    pub(super) fn instance(&mut self, func: Id<Func>, type_args: Vec<TypeId>) -> FuncId {
        // A trait's method, for a known `Self`: the method its implementation has.
        if let Some((method, args)) = self.dispatch(func, &type_args) {
            return self.instance(method, args);
        }
        // Or one numbers have built in: `compare` of `i32`.
        if let Some(id) = self.builtin_method(func, &type_args) {
            return id;
        }
        let key = (func, type_args);
        if let Some(&id) = self.instances.get(&key) {
            return id;
        }
        let (_, type_args) = &key;
        let ast_func = &self.ast.funcs[func];
        let def = self.names.func(func);

        let subst = Subst::new(self.names.type_params(func), type_args, &self.typed.impls);
        let sig = self.typed.func(func);
        let params = sig
            .params
            .iter()
            .map(|&p| subst.apply(&mut self.mir.types, p))
            .collect();
        let ret = subst.apply(&mut self.mir.types, sig.ret);

        let mut name = ast_func.name.name.to_string();
        if let Some(owner) = self.owners.get(&func) {
            name = format!("{owner}::{name}");
        }
        name = self.qualified(def, &name);
        for &arg in type_args {
            self.make_structs_in(arg);
        }
        let name = self.with_type_args(name, type_args);

        let is_entry = &*ast_func.name.name == som_lang::ENTRY_POINT
            && self.ast.modules[self.names.def(def).module].name.is_empty()
            && !self.owners.contains_key(&func)
            && type_args.is_empty();

        let id = self.mir.funcs.alloc(Function {
            name,
            params,
            ret,
            span: self.ast.funcs.span(func).clone(),
            body: None,
            link_name: ast_func.body.is_none().then(|| {
                ast_func
                    .link_name
                    .as_deref()
                    .unwrap_or(&ast_func.name.name)
                    .to_string()
            }),
            library: ast_func.library.clone(),
            is_entry,
            variadic: ast_func.variadic,
        });
        if ast_func.body.is_some() {
            self.queue.push((id, Job::Func(func, type_args.clone())));
        }
        if let Some(at) = &self.at {
            self.requested_at.insert(id, at.clone());
        }
        self.instances.insert(key, id);
        id
    }

    /// `c`'s initializer, run while compiling. One in a generic function can use its
    /// parameters, so there's one for each instance.
    pub(super) fn init(&mut self, c: Id<som_ast::Const>, owner: Owner) -> FuncId {
        let owner = match owner {
            Some((_, args)) if args.is_empty() => None,
            _ if self.item_consts.contains(&c) => None,
            owner => owner,
        };
        let key = (c, owner);
        if let Some(&id) = self.inits.get(&key) {
            return id;
        }
        let (_, owner) = &key;
        let constant = &self.ast.consts[c];
        let def = self.names.const_def(c);
        let subst = self.owner_subst(owner);
        let ret = self.typed_for(owner).expr(constant.value);
        let ret = subst.apply(&mut self.mir.types, ret);
        self.make_structs_in(ret);
        // `@const SIZE<4>`, or just `@const` for one without a name.
        let keyword = Builtin::Const.name();
        let name = match constant.is_named() {
            false => keyword.to_string(),
            true => format!("{keyword} {}", self.qualified(def, &constant.name.name)),
        };
        let args = owner.as_ref().map_or(&[][..], |(_, args)| args);
        let name = self.with_type_args(name, args);
        let span = self.ast.consts.span(c).clone();
        let id = (self.mir.funcs).alloc(internal_function(name, Vec::new(), ret, span));
        self.queue.push((id, Job::Init(c, owner.clone())));
        self.inits.insert(key, id);
        // Even if nothing uses its value: `@assert(…);` in a function.
        self.mir.checks.push(id);
        id
    }

    /// For a function whose `@if`s depend on its generic parameters: what's found is used to
    /// lower it. False if it's wrong.
    pub(super) fn check_instance(&mut self, id: FuncId, func: Id<Func>, args: &[TypeId]) -> bool {
        let key = (func, args.to_vec());
        if self.instance_typed.contains_key(&key) {
            return true;
        }
        let regions = self.typed.region_spans(self.ast);
        let (typed, errors) = som_typeck::check_instance(
            self.ast,
            self.names,
            self.typed.const_values(),
            self.mir.types.clone(),
            func,
            args,
            &regions,
        );
        // Its types were added to lowering's: the ids lowering has are still theirs.
        self.mir.types = typed.table.clone();
        if !errors.is_empty() {
            let used_at = (self.requested_at.get(&id).cloned())
                .unwrap_or_else(|| self.ast.funcs.span(func).clone());
            self.instance_errors.push(InstanceErrors {
                name: self.mir.funcs[id].name.clone(),
                used_at,
                errors,
            });
            return false;
        }
        self.instance_typed.insert(key, Rc::new(typed));
        true
    }

    pub(super) fn typed_for(&self, owner: &Owner) -> TypedRef<'a> {
        match owner.as_ref().and_then(|key| self.instance_typed.get(key)) {
            Some(typed) => TypedRef::Instance(typed.clone()),
            None => TypedRef::Global(self.typed),
        }
    }

    pub(super) fn owner_subst(&self, owner: &Owner) -> Subst<'a> {
        match owner {
            Some((func, args)) => {
                Subst::new(self.names.type_params(*func), args, &self.typed.impls)
            }
            None => Subst::new(&[], &[], &self.typed.impls),
        }
    }
}

/// Also works out the associated types that makes known: `I::Item` with `I` a
/// `ListIntoIter<i32>` is `i32`.
pub(super) struct Subst<'a> {
    map: Vec<(DefId, TypeId)>,
    impls: &'a [ImplInfo],
}

impl<'a> Subst<'a> {
    pub(super) fn new(params: &[DefId], args: &[TypeId], impls: &'a [ImplInfo]) -> Subst<'a> {
        Subst {
            map: params.iter().copied().zip(args.iter().copied()).collect(),
            impls,
        }
    }

    pub(super) fn apply(&self, types: &mut TypeTable, ty: TypeId) -> TypeId {
        let replaced = self.replace(types, ty);
        resolve_assoc(types, self.impls, replaced)
    }

    fn replace(&self, types: &mut TypeTable, ty: TypeId) -> TypeId {
        if self.map.is_empty() {
            return ty;
        }
        match types.get(ty).clone() {
            Ty::Param(def) => self
                .map
                .iter()
                .find(|(d, _)| *d == def)
                .map_or(ty, |(_, t)| *t),
            other if other.children().is_empty() => ty,
            other => {
                let applied = other.map_children(|child| self.replace(types, child));
                let applied = types.intern(applied);
                // `N + 1` with `N` filled in adds up: `3 + 1` is `4`.
                types.normalize(applied)
            }
        }
    }
}

/// A function of the compiler's own: not extern, not the entry point.
pub(super) fn internal_function(
    name: String,
    params: Vec<TypeId>,
    ret: TypeId,
    span: Span,
) -> Function {
    Function {
        name,
        params,
        ret,
        span,
        body: None,
        link_name: None,
        library: None,
        is_entry: false,
        variadic: false,
    }
}
