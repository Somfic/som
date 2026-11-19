//! The first phase: every module's items and every impl block's members, defined before any
//! code is resolved, so items can be used before they're written.

use som_ast::{Alias, Func, Id, Ident, Impl, Item, Struct, Trait, TypeExpr};
use som_lang::{LangFunc, LangStruct, LangTrait};
use std::collections::HashSet;

use crate::names::{DefId, DefKind, ImplDef};
use crate::{Expect, ResolveError, Resolver};

impl<'a> Resolver<'a> {
    pub(crate) fn collect_modules(&mut self) {
        for (index, module) in self.ast.modules.iter().enumerate() {
            self.modules.by_name.insert(&module.name, index);
        }
        for (index, module) in self.ast.modules.iter().enumerate() {
            for item in &module.items {
                let Item::Use(u) = item else { continue };
                let Some(&used) = self.modules.by_name.get(u.path.to_string().as_str()) else {
                    continue;
                };
                if !self.modules.imports[index].contains(&used) {
                    self.modules.imports[index].push(used);
                }
                if u.public && !self.modules.reexports[index].contains(&used) {
                    self.modules.reexports[index].push(used);
                }
            }
        }
    }

    pub(crate) fn collect_items(&mut self) {
        let ast = self.ast;
        for (m, module) in ast.modules.iter().enumerate() {
            for item in &module.items {
                match item {
                    Item::Func(id) => self.define_func(*id, m, true),
                    // A library's functions belong to its module, wherever they're declared.
                    Item::Extern(block) => {
                        for &id in &block.funcs {
                            self.define_func(id, block.module.unwrap_or(m), true);
                        }
                    }
                    Item::Struct(id) => self.collect_struct(*id, m),
                    Item::Trait(id) => self.collect_trait(*id, m),
                    // Methods aren't visible by name in the module; they're attached in
                    // `collect_methods`.
                    Item::Impl(block) => {
                        for &id in &block.methods {
                            self.define_func(id, m, false);
                        }
                    }
                    Item::Alias(alias) => self.collect_alias(alias, m),
                    Item::Const(id) => self.collect_const(*id, m),
                    Item::Use(_) => {}
                }
            }
        }
        self.bind_lang_items();
        self.collect_listeners();
        self.collect_tests();
    }

    fn collect_struct(&mut self, id: Id<Struct>, m: usize) {
        let s = &self.ast.structs[id];
        let def = self.define_at(&s.name, DefKind::Struct(id), m);
        self.names.set_struct(id, def);
        self.add_item(m, &s.name, def);
        for (index, v) in s.variants().iter().enumerate() {
            let kind = DefKind::Variant { strukt: id, index };
            let variant = self.define_at(&v.name, kind, m);
            self.names.push_variant(id, variant);
            self.modules.variant_names[m].insert(&v.name.name);
        }
    }

    fn collect_trait(&mut self, id: Id<Trait>, m: usize) {
        let t = &self.ast.traits[id];
        let def = self.define_at(&t.name, DefKind::Trait(id), m);
        let types = (t.types.iter().enumerate())
            .map(|(index, assoc)| {
                self.define_at(&assoc.name, DefKind::AssocType { trait_: id, index }, m)
            })
            .collect();
        self.names.set_trait(id, def, types);
        self.add_item(m, &t.name, def);
        // Its methods are only reached through types implementing it.
        for &method in &t.methods {
            self.define_func(method, m, false);
        }
    }

    fn collect_alias(&mut self, alias: &'a Alias, m: usize) {
        let def = self.define_at(&alias.name, DefKind::Alias { ty: alias.ty }, m);
        let params = (alias.generics.iter().enumerate())
            .map(|(index, g)| {
                let value = g.ty.is_some();
                self.define_at(&g.name, DefKind::AliasParam { index, value }, m)
            })
            .collect();
        self.names.add_alias(def, params);
        self.add_item(m, &alias.name, def);
    }

    fn collect_const(&mut self, id: Id<som_ast::Const>, m: usize) {
        let c = &self.ast.consts[id];
        let def = self.define_at(&c.name, DefKind::Const(id), m);
        self.names.set_const(id, def);
        // `@const assert(…);` is run for what it checks: nothing names it.
        if c.is_named() {
            self.add_item(m, &c.name, def);
        }
    }

    fn bind_lang_items(&mut self) {
        let Some(&root) = self.modules.by_name.get(som_lang::PRELUDE_MODULE) else {
            return;
        };
        // `som` and what it `pub use`s: `pub use som::iter` brings in `Iterator`.
        let scope = self.modules.with_reexports(std::iter::once(root));
        let find = |this: &Self, name: &str, fits: fn(&DefKind) -> bool| {
            (scope.iter())
                .filter_map(|&m| this.modules.items[m].get(name).copied())
                .find(|&def| fits(&this.names.def(def).kind))
        };
        for &lang in LangTrait::ALL {
            if let Some(def) = find(self, lang.name(), |k| matches!(k, DefKind::Trait(_))) {
                self.names.set_lang(lang, def);
            }
        }
        for &lang in LangStruct::ALL {
            if let Some(def) = find(self, lang.name(), |k| matches!(k, DefKind::Struct(_))) {
                self.names.set_lang_struct(lang, def);
            }
        }
        for &lang in LangFunc::ALL {
            if let Some(def) = find(self, lang.name(), |k| matches!(k, DefKind::Func(_))) {
                self.names.set_lang_func(lang, def);
            }
        }
    }

    /// The functions `@on fail` says are called when the program fails: the program's own,
    /// in order, or if it has none, the standard library's.
    fn collect_listeners(&mut self) {
        let ast = self.ast;
        let methods: HashSet<Id<Func>> = (ast.modules.iter())
            .flat_map(|module| &module.items)
            .filter_map(|item| match item {
                Item::Impl(block) => Some(block.methods.iter().copied()),
                _ => None,
            })
            .flatten()
            .collect();
        let (mut own, mut std) = (Vec::new(), Vec::new());
        for (id, func) in ast.funcs.iter() {
            for event in &func.listens {
                if &*event.name != som_lang::FAIL_EVENT {
                    self.errors.push(ResolveError::UnknownEvent {
                        name: event.name.to_string(),
                        span: event.span.clone(),
                    });
                    continue;
                }
                // Called with what failed, by the program: a function of its own.
                if methods.contains(&id) || func.in_trait {
                    self.errors.push(ResolveError::ListenerIsMethod {
                        span: func.name.span.clone(),
                    });
                    continue;
                }
                let module = &ast.modules[self.names.def(self.names.func(id)).module].name;
                match som_lang::is_std_module(module) {
                    true => std.push(id),
                    false => own.push(id),
                }
            }
        }
        self.names
            .set_fail_listeners(if own.is_empty() { std } else { own });
    }

    /// The `@test` functions. A test is a function of its own: the runner calls it, nothing
    /// else does.
    fn collect_tests(&mut self) {
        let ast = self.ast;
        let methods: HashSet<Id<Func>> = (ast.modules.iter())
            .flat_map(|module| &module.items)
            .filter_map(|item| match item {
                Item::Impl(block) => Some(block.methods.iter().copied()),
                _ => None,
            })
            .flatten()
            .collect();
        let mut tests = Vec::new();
        for (id, func) in ast.funcs.iter().filter(|(_, func)| func.test) {
            match methods.contains(&id) || func.in_trait {
                true => self.errors.push(ResolveError::TestIsMethod {
                    span: func.name.span.clone(),
                }),
                false => tests.push(id),
            }
        }
        self.names.set_tests(tests);
    }

    fn define_func(&mut self, id: Id<Func>, module: usize, visible: bool) {
        let func = &self.ast.funcs[id];
        let is_extern = func.body.is_none();

        // Declaring the same extern function twice is fine: both name the same symbol.
        if visible
            && is_extern
            && let Some(&existing) = self.modules.items[module].get(&*func.name.name)
            && let DefKind::Func(other) = self.names.def(existing).kind
            && self.ast.funcs[other].body.is_none()
        {
            self.names.set_func(id, existing);
            return;
        }

        let def = self.define_at(&func.name, DefKind::Func(id), module);
        self.names.set_func(id, def);
        if visible {
            self.add_item(module, &func.name, def);
        }
    }

    fn add_item(&mut self, module: usize, name: &'a Ident, def: DefId) {
        let Some(&previous) = self.modules.items[module].get(&*name.name) else {
            self.modules.items[module].insert(&name.name, def);
            return;
        };
        // Functions can share a name: a call picks the one whose types fit.
        if matches!(self.names.def(previous).kind, DefKind::Func(_))
            && matches!(self.names.def(def).kind, DefKind::Func(_))
        {
            self.names.add_overload(previous, def);
            return;
        }
        self.duplicate(name, previous);
    }

    pub(crate) fn collect_methods(&mut self) {
        let ast = self.ast;
        for (m, module) in ast.modules.iter().enumerate() {
            self.cx.module = m;
            for item in &module.items {
                if let Item::Impl(block) = item {
                    self.collect_impl(block);
                }
            }
        }
    }

    fn collect_impl(&mut self, block: &'a Impl) {
        let index = self.names.impls.len();
        // `for C`, one of its own parameters: for whatever meets its bounds.
        let blanket_at = (block.trait_.is_some() && block.target_ty.is_none())
            .then(|| match &block.target.segments[..] {
                [only] if block.target_args.is_empty() => {
                    (block.generics.iter()).position(|g| g.name.name == only.name)
                }
                _ => None,
            })
            .flatten();
        let owner = match blanket_at {
            Some(_) => None,
            None => self.impl_owner(block),
        };
        let trait_ =
            (block.trait_.as_ref()).and_then(|(path, _)| self.resolve_path(path, Expect::Trait));
        let params = (block.generics.iter().enumerate())
            .map(|(i, g)| {
                let kind = DefKind::ImplParam {
                    impl_: index,
                    index: i,
                    value: g.ty.is_some(),
                };
                self.define(&g.name, kind)
            })
            .collect::<Vec<DefId>>();
        let types = (block.types.iter().enumerate())
            .map(|(i, (name, _))| {
                let kind = DefKind::ImplType {
                    impl_: index,
                    index: i,
                };
                self.define(name, kind)
            })
            .collect();
        let blanket = blanket_at.map(|at| params[at]);
        self.names.add_impl(ImplDef {
            module: self.cx.module,
            trait_,
            owner,
            params,
            methods: block.methods.clone(),
            types,
            blanket,
        });
        self.collect_impl_consts(block, owner);
        if let Some(owner) = owner {
            self.collect_impl_methods(block, owner);
        }
    }

    /// The type an impl block gives methods of its own. Only a struct or a built-in type has
    /// them; an implementation for `[i32; 3]` is found through its trait.
    fn impl_owner(&mut self, block: &Impl) -> Option<DefId> {
        let Some(ty) = block.target_ty else {
            return self.resolve_path(&block.target, Expect::Struct);
        };
        match self.ast.types[ty] {
            TypeExpr::Primitive(p) => Some(self.names.primitive_or_define(
                p,
                block.target.span(),
                self.cx.module,
            )),
            _ if block.trait_.is_some() => None,
            _ => {
                self.errors.push(ResolveError::MethodsOnType {
                    ty: block.target.to_string(),
                    span: block.target.span(),
                });
                None
            }
        }
    }

    /// `@const PI = 3.14;`: named through the type.
    fn collect_impl_consts(&mut self, block: &Impl, owner: Option<DefId>) {
        let ast = self.ast;
        for &c in &block.consts {
            let name = &ast.consts[c].name;
            let def = self.define(name, DefKind::Const(c));
            self.names.set_const(c, def);
            let Some(owner) = owner else { continue };
            let previous = (self.names.variant(owner, &name.name))
                .or_else(|| self.names.method(owner, &name.name))
                .or_else(|| self.names.add_assoc_const(owner, c, &name.name, def));
            if let Some(previous) = previous {
                self.duplicate(name, previous);
            }
        }
    }

    fn collect_impl_methods(&mut self, block: &Impl, owner: DefId) {
        let ast = self.ast;
        for &method in &block.methods {
            let name = &ast.funcs[method].name;
            let def = self.names.func(method);
            if ast.funcs[method].prop {
                self.collect_prop(owner, block, method, def);
                continue;
            }
            if let Some(constant) = self.names.assoc_const(owner, &name.name) {
                self.duplicate(name, constant);
                continue;
            }
            // Methods can share a name, like functions: `get` taking `&self` and
            // `&mut self`. The type checker picks the one that fits.
            if let Some(previous) = self.names.add_method(owner, &name.name, def) {
                self.names.add_overload(previous, def);
            }
        }
    }

    /// Adds property `prop` to `owner`, checking it's one: no type parameters of its own, and
    /// no field named like it, unless the field is less visible.
    fn collect_prop(&mut self, owner: DefId, block: &Impl, prop: Id<Func>, def: DefId) {
        let ast = self.ast;
        let func = &ast.funcs[prop];
        let name = &func.name;
        if func.generics.len() > block.generics.len() {
            let own = &func.generics[block.generics.len()..];
            let span = own[0].name.span.merge(&own[own.len() - 1].name.span);
            self.errors.push(ResolveError::PropGenerics { span });
        }
        // A trait's is as visible as the trait.
        let visibility = match block.trait_ {
            Some(_) => som_ast::Visibility::Public,
            None => func.visibility,
        };
        if let DefKind::Struct(s) = self.names.def(owner).kind {
            let strukt = &ast.structs[s];
            // A private field and a `pub prop` of its name is a field only some code can
            // change: that code reads the field, the rest the property. As visible as the
            // field, the property could never be reached.
            let field = (strukt.fields.iter().chain(strukt.payload()))
                .find(|f| f.name.name == name.name)
                .filter(|f| strukt.is_enum() || f.visibility >= visibility);
            if let Some(field) = field {
                self.errors.push(ResolveError::PropNamedLikeField {
                    name: name.name.to_string(),
                    span: name.span.clone(),
                    field: field.name.span.clone(),
                });
            }
        }
        let block = self
            .names
            .impl_of(prop)
            .expect("properties are in impl blocks");
        let conditional = |kind: &DefKind| match *kind {
            DefKind::Func(f) => !ast.funcs[f].when.is_empty(),
            _ => false,
        };
        if let Some(previous) = self
            .names
            .add_prop(owner, &name.name, block, def, conditional)
        {
            self.duplicate(name, previous);
        }
    }
}
