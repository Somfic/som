//! Name resolution: binds every name in the syntax tree to the definition it
//! refers to. This is the only place in the compiler that looks names up by
//! string; every later pass works with `DefId`s.
//!
//! Lookup rules:
//! - An unqualified name is looked up in the enclosing local scopes, then in the
//!   current module, then in the modules it `use`s, in order.
//! - A qualified name starts with a module (`math::add`, any loaded module) or a
//!   struct (`Point::new`, for methods), or both (`geo::Point::new`).
//! - Items can be used before they are defined; locals cannot.

mod error;
mod names;
pub mod visibility;

pub use error::ResolveError;
pub use names::{Def, DefId, DefKind, ImplDef, Names};
use som_lang::{LangFunc, LangStruct, LangTrait};

use som_ast::{Ast, Expr, Func, Id, Ident, Item, Path, Pattern, PatternFields, Stmt, TypeExpr};
use som_base::diagnostics::{closest_match, snake_case};
use som_base::foreign;
use som_base::message::{self, Message, Part, code};
use som_base::{Fix, msg};
use std::collections::HashMap;
use std::collections::HashSet;

pub fn resolve(ast: &Ast) -> (Names, Vec<ResolveError>) {
    let mut resolver = Resolver {
        ast,
        names: Names::new(ast),
        errors: Vec::new(),
        modules: HashMap::new(),
        items: vec![HashMap::new(); ast.modules.len()],
        imports: vec![Vec::new(); ast.modules.len()],
        block_imports: Vec::new(),
        variant_names: vec![HashSet::new(); ast.modules.len()],
        module: 0,
        scopes: Vec::new(),
        const_depth: None,
        settled_condition: false,
        type_params: Vec::new(),
        block_types: Vec::new(),
        through_alias: std::cell::Cell::new(None),
        owner: None,
        libraries: HashMap::new(),
        declared: vec![HashSet::new(); ast.modules.len()],
    };
    // A library's module, `c`, is named only where its `extern "c"` is written.
    for (m, module) in ast.modules.iter().enumerate() {
        for item in &module.items {
            if let Item::Extern(block) = item
                && let (Some(library), Some(lib)) = (&block.library, block.module)
            {
                resolver.libraries.insert(lib, library.as_str());
                resolver.declared[m].insert(lib);
            }
        }
    }
    resolver.collect_modules();
    resolver.collect_items();
    resolver.collect_methods();
    resolver.resolve_bodies();
    (resolver.names, resolver.errors)
}

/// What kind of thing a path is expected to name. Decides where to look and which error to report.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    Value,
    Func,
    Struct,
    Type,
    Trait,
}

impl Expect {
    /// `name` tagged as the kind of thing this expects, for messages.
    fn name(self, name: &str) -> Part {
        match self {
            Expect::Value => message::var(name),
            Expect::Func => message::func(name),
            Expect::Struct => message::struct_name(name),
            Expect::Type | Expect::Trait => message::ty(name),
        }
    }
}

struct Resolver<'a> {
    ast: &'a Ast,
    /// Resolving an `@if`'s condition, where a type can be compared with another: `T == f32`.
    settled_condition: bool,
    names: Names,
    errors: Vec<ResolveError>,
    /// Module name → index into `ast.modules`.
    modules: HashMap<&'a str, usize>,
    /// Per module: its functions and structs, by name.
    items: Vec<HashMap<&'a str, DefId>>,
    /// Per module: the modules it `use`s, in order.
    imports: Vec<Vec<usize>>,
    /// The modules `use`d inside the blocks being resolved, with how many scopes deep each
    /// `use` is: gone when its block ends.
    block_imports: Vec<(usize, usize)>,
    /// Per module: the names of its enums' variants. A bare name in a pattern that's one of
    /// those, here or in a module this one uses, is that variant rather than a new binding.
    variant_names: Vec<HashSet<&'a str>>,
    /// The module whose code is being resolved.
    module: usize,
    /// Libraries' modules → the library: `c` for `c::puts`.
    libraries: HashMap<usize, &'a str>,
    /// Per module: the libraries' modules it has an `extern` for, which it can name.
    declared: Vec<HashSet<usize>>,
    /// Local scopes of the function being resolved, innermost last.
    scopes: Vec<HashMap<&'a str, DefId>>,
    /// Type parameters of the function being resolved.
    type_params: Vec<(&'a str, DefId)>,
    /// Types named inside a trait or impl block besides its parameters: `Self`, and its
    /// associated types, `Item`.
    block_types: Vec<(&'a str, DefId)>,
    /// The alias the last path looked up named its struct through, if it did.
    through_alias: std::cell::Cell<Option<DefId>>,
    /// The type whose impl block is being resolved: what its members' privacy is to.
    owner: Option<DefId>,
    /// Resolving a `@const` inside a function: how many scopes were open where it starts.
    /// Its value can't name the locals in those, only its own.
    const_depth: Option<usize>,
}

impl<'a> Resolver<'a> {
    // --- Phase 1: modules and items ---

    fn collect_modules(&mut self) {
        for (index, module) in self.ast.modules.iter().enumerate() {
            self.modules.insert(&module.name, index);
        }
        for (index, module) in self.ast.modules.iter().enumerate() {
            for item in &module.items {
                if let Item::Use(u) = item
                    && let Some(&used) = self.modules.get(u.path.to_string().as_str())
                    && !self.imports[index].contains(&used)
                {
                    self.imports[index].push(used);
                }
            }
        }
    }

    fn collect_items(&mut self) {
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
                    Item::Struct(id) => {
                        let s = &ast.structs[*id];
                        let def = self.names.define(
                            &s.name.name,
                            DefKind::Struct(*id),
                            s.name.span.clone(),
                            m,
                        );
                        self.names.set_struct(*id, def);
                        self.add_item(m, &s.name, def);
                        // Named through the enum: `Shape::Circle`.
                        for (index, v) in s.variants().iter().enumerate() {
                            let kind = DefKind::Variant { strukt: *id, index };
                            let variant =
                                self.names
                                    .define(&v.name.name, kind, v.name.span.clone(), m);
                            self.names.push_variant(*id, variant);
                            self.variant_names[m].insert(&v.name.name);
                        }
                    }
                    Item::Trait(id) => {
                        let t = &ast.traits[*id];
                        let def = (self.names).define(
                            &t.name.name,
                            DefKind::Trait(*id),
                            t.name.span.clone(),
                            m,
                        );
                        let types = (t.types.iter().enumerate())
                            .map(|(index, assoc)| {
                                let kind = DefKind::AssocType { trait_: *id, index };
                                self.names.define(
                                    &assoc.name.name,
                                    kind,
                                    assoc.name.span.clone(),
                                    m,
                                )
                            })
                            .collect();
                        self.names.set_trait(*id, def, types);
                        self.add_item(m, &t.name, def);
                        // Its methods are only reached through types implementing it.
                        for &method in &t.methods {
                            self.define_func(method, m, false);
                        }
                    }
                    Item::Impl(block) => {
                        // Methods aren't visible by name in the module; they're attached in `collect_methods`.
                        for &id in &block.methods {
                            self.define_func(id, m, false);
                        }
                    }
                    Item::Alias(alias) => {
                        let kind = DefKind::Alias { ty: alias.ty };
                        let def =
                            (self.names).define(&alias.name.name, kind, alias.name.span.clone(), m);
                        let params = (alias.generics.iter().enumerate())
                            .map(|(index, g)| {
                                let kind = DefKind::AliasParam {
                                    index,
                                    value: g.ty.is_some(),
                                };
                                self.names
                                    .define(&g.name.name, kind, g.name.span.clone(), m)
                            })
                            .collect();
                        self.names.add_alias(def, params);
                        self.add_item(m, &alias.name, def);
                    }
                    Item::Const(id) => {
                        let c = &ast.consts[*id];
                        let def = (self.names).define(
                            &c.name.name,
                            DefKind::Const(*id),
                            c.name.span.clone(),
                            m,
                        );
                        self.names.set_const(*id, def);
                        // `@const assert(…);` is run for what it checks: nothing names it.
                        if c.is_named() {
                            self.add_item(m, &c.name, def);
                        }
                    }
                    Item::Use(_) => {}
                }
            }
        }
        self.bind_lang_items();
        self.collect_listeners();
    }

    fn bind_lang_items(&mut self) {
        let Some(&root) = self.modules.get(som_lang::PRELUDE_MODULE) else {
            return;
        };
        let find = |this: &Self, name: &str, fits: fn(&DefKind) -> bool| {
            (this.items[root].get(name).copied()).filter(|&def| fits(&this.names.def(def).kind))
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

    fn define_func(&mut self, id: Id<Func>, module: usize, visible: bool) {
        let func = &self.ast.funcs[id];
        let is_extern = func.body.is_none();

        // Declaring the same extern function twice is fine: both name the same symbol.
        if visible
            && is_extern
            && let Some(&existing) = self.items[module].get(&*func.name.name)
            && let DefKind::Func(other) = self.names.def(existing).kind
            && self.ast.funcs[other].body.is_none()
        {
            self.names.set_func(id, existing);
            return;
        }

        let def = self.names.define(
            &func.name.name,
            DefKind::Func(id),
            func.name.span.clone(),
            module,
        );
        self.names.set_func(id, def);
        if visible {
            self.add_item(module, &func.name, def);
        }
    }

    fn add_item(&mut self, module: usize, name: &'a Ident, def: DefId) {
        // Functions can share a name: a call picks the one whose types fit.
        if let Some(&previous) = self.items[module].get(&*name.name)
            && matches!(self.names.def(previous).kind, DefKind::Func(_))
            && matches!(self.names.def(def).kind, DefKind::Func(_))
        {
            self.names.add_overload(previous, def);
            return;
        }
        if let Some(&previous) = self.items[module].get(&*name.name) {
            self.errors.push(ResolveError::DuplicateDefinition {
                name: name.name.to_string(),
                span: name.span.clone(),
                previous: self.names.def(previous).span.clone(),
            });
            return;
        }
        self.items[module].insert(&name.name, def);
    }

    fn collect_methods(&mut self) {
        let ast = self.ast;
        for (m, module) in ast.modules.iter().enumerate() {
            self.module = m;
            for item in &module.items {
                let Item::Impl(block) = item else { continue };
                let index = self.names.impls.len();
                // Only a struct has methods of its own; an implementation for `bool` is found
                // through its trait.
                let owner = match block.target_ty {
                    // `impl i32 { … }` and `impl Double for i32 { … }`: a built-in type's
                    // methods, `i32::double`, as a struct's are.
                    Some(ty) => match ast.types[ty] {
                        som_ast::TypeExpr::Primitive(p) => {
                            Some(self.names.primitive_or_define(p, block.target.span(), m))
                        }
                        _ if block.trait_.is_some() => None,
                        _ => {
                            self.errors.push(ResolveError::MethodsOnType {
                                ty: block.target.to_string(),
                                span: block.target.span(),
                            });
                            None
                        }
                    },
                    None => self.resolve_path(&block.target, Expect::Struct),
                };
                let trait_ = (block.trait_.as_ref())
                    .and_then(|(path, _)| self.resolve_path(path, Expect::Trait));
                let params = (block.generics.iter().enumerate())
                    .map(|(i, g)| {
                        let kind = DefKind::ImplParam {
                            impl_: index,
                            index: i,
                            value: g.ty.is_some(),
                        };
                        self.names
                            .define(&g.name.name, kind, g.name.span.clone(), m)
                    })
                    .collect();
                let types = (block.types.iter().enumerate())
                    .map(|(i, (name, _))| {
                        let kind = DefKind::ImplType {
                            impl_: index,
                            index: i,
                        };
                        self.names.define(&name.name, kind, name.span.clone(), m)
                    })
                    .collect();
                self.names.add_impl(ImplDef {
                    module: m,
                    trait_,
                    owner,
                    params,
                    methods: block.methods.clone(),
                    types,
                });
                // `@const PI = 3.14;`: named through the type.
                for &c in &block.consts {
                    let name = &ast.consts[c].name;
                    let def =
                        (self.names).define(&name.name, DefKind::Const(c), name.span.clone(), m);
                    self.names.set_const(c, def);
                    let Some(owner) = owner else { continue };
                    let previous = (self.names.variant(owner, &name.name))
                        .or_else(|| self.names.method(owner, &name.name))
                        .or_else(|| self.names.add_assoc_const(owner, c, &name.name, def));
                    if let Some(previous) = previous {
                        self.errors.push(ResolveError::DuplicateDefinition {
                            name: name.name.to_string(),
                            span: name.span.clone(),
                            previous: self.names.def(previous).span.clone(),
                        });
                    }
                }
                let Some(owner) = owner else {
                    continue;
                };
                for &method in &block.methods {
                    let name = &ast.funcs[method].name;
                    let def = self.names.func(method);
                    if ast.funcs[method].prop {
                        self.collect_prop(owner, block, method, def);
                        continue;
                    }
                    if let Some(constant) = self.names.assoc_const(owner, &name.name) {
                        self.errors.push(ResolveError::DuplicateDefinition {
                            name: name.name.to_string(),
                            span: name.span.clone(),
                            previous: self.names.def(constant).span.clone(),
                        });
                        continue;
                    }
                    // Methods can share a name, like functions: `get` taking `&self` and
                    // `&mut self`. The type checker picks the one that fits.
                    if let Some(previous) = self.names.add_method(owner, &name.name, def) {
                        self.names.add_overload(previous, def);
                    }
                }
            }
        }
    }

    /// Adds property `prop` to `owner`, checking it's one: no type parameters of its own, and
    /// no field named like it, unless the field is less visible. Its only parameter, `&self`, isn't written.
    fn collect_prop(&mut self, owner: DefId, block: &som_ast::Impl, prop: Id<Func>, def: DefId) {
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
            self.errors.push(ResolveError::DuplicateDefinition {
                name: name.name.to_string(),
                span: name.span.clone(),
                previous: self.names.def(previous).span.clone(),
            });
        }
    }

    // --- Phase 2: bodies ---

    fn resolve_bodies(&mut self) {
        let ast = self.ast;
        self.resolve_aliases();
        // Impl blocks, counted in the order `collect_methods` saw them.
        let mut impl_index = 0;
        for (m, module) in ast.modules.iter().enumerate() {
            self.module = m;
            for item in &module.items {
                match item {
                    Item::Func(id) => self.resolve_func(*id),
                    Item::Extern(block) => block.funcs.iter().for_each(|&id| self.resolve_func(id)),
                    Item::Impl(block) => {
                        let index = impl_index;
                        impl_index += 1;
                        self.resolve_impl(index, block);
                    }
                    Item::Trait(id) => self.resolve_trait(*id),
                    Item::Struct(id) => {
                        // Its fields can be written in terms of its type parameters.
                        self.type_params.clear();
                        for (index, param) in ast.structs[*id].generics.iter().enumerate() {
                            let kind = DefKind::StructParam {
                                strukt: *id,
                                index,
                                value: param.ty.is_some(),
                            };
                            let def = self.names.define(
                                &param.name.name,
                                kind,
                                param.name.span.clone(),
                                self.module,
                            );
                            self.names.push_struct_param(*id, def);
                            self.type_params.push((&param.name.name, def));
                        }
                        let defs = self.names.struct_params(*id).to_vec();
                        for (param, def) in ast.structs[*id].generics.iter().zip(defs) {
                            if let Some(ty) = param.ty {
                                self.resolve_type(ty);
                            }
                            if let Some(default) = param.default {
                                self.resolve_type(default);
                            }
                            self.resolve_bounds(param, def);
                        }
                        for field in ast.structs[*id]
                            .fields
                            .iter()
                            .chain(ast.structs[*id].payload())
                        {
                            self.resolve_type(field.ty);
                        }
                        self.type_params.clear();
                    }
                    Item::Alias(_) => {}
                    Item::Const(id) => {
                        let c = &ast.consts[*id];
                        self.type_params.clear();
                        if let Some(ty) = c.ty {
                            self.resolve_type(ty);
                        }
                        self.scopes.push(HashMap::new());
                        self.resolve_expr(c.value);
                        self.pop_scope();
                    }
                    Item::Use(_) => {}
                }
            }
        }
        self.check_consts();
    }

    /// A constant can't be made of itself. One whose value runs code is worked out by
    /// running it, which finds a loop through functions then.
    fn check_consts(&mut self) {
        let ast = self.ast;
        for (id, c) in ast.consts.iter() {
            // Following the constants it names, it mustn't come back to itself.
            let mut seen = HashSet::new();
            let mut todo = vec![c.value];
            while let Some(expr) = todo.pop() {
                for used in consts_in(ast, &self.names, expr) {
                    if used == id {
                        self.errors.push(ResolveError::ConstCycle {
                            name: c.name.name.to_string(),
                            span: c.name.span.clone(),
                        });
                        todo.clear();
                        break;
                    }
                    if seen.insert(used) {
                        todo.push(ast.consts[used].value);
                    }
                }
            }
        }
    }

    /// What each type alias stands for, before any code, which can name a struct through one:
    /// `Vector3::zero()`.
    fn resolve_aliases(&mut self) {
        let ast = self.ast;
        // In the order `collect_items` saw them.
        let mut alias_index = 0;
        for (m, module) in ast.modules.iter().enumerate() {
            self.module = m;
            for item in &module.items {
                let Item::Alias(alias) = item else {
                    continue;
                };
                // Its type can be written in terms of its parameters.
                let def = self.names.aliases()[alias_index];
                alias_index += 1;
                let params = self.names.alias_params(def).to_vec();
                self.type_params.clear();
                for (g, &param) in alias.generics.iter().zip(&params) {
                    self.type_params.push((&g.name.name, param));
                }
                for (g, &param) in alias.generics.iter().zip(&params) {
                    if let Some(ty) = g.ty {
                        self.resolve_type(ty);
                    }
                    self.resolve_bounds(g, param);
                }
                self.resolve_type(alias.ty);
                self.type_params.clear();
            }
        }
    }

    /// An impl block's target, trait and associated types, in terms of its parameters; then
    /// its methods, which can name `Self` and the associated types.
    fn resolve_impl(&mut self, index: usize, block: &'a som_ast::Impl) {
        let params = self.names.impls[index].params.clone();
        self.type_params = (block.generics.iter().zip(&params))
            .map(|(g, &def)| (&*g.name.name, def))
            .collect();
        for (g, &def) in block.generics.iter().zip(&params) {
            if let Some(ty) = g.ty {
                self.resolve_type(ty);
            }
            self.resolve_bounds(g, def);
        }
        let written = (block.target_args.iter())
            .chain(&block.target_ty)
            .chain(block.trait_.iter().flat_map(|(_, args)| args))
            .chain(block.types.iter().map(|(_, ty)| ty));
        for &ty in written {
            self.resolve_type(ty);
        }
        // `@if N >= 3 { … }` among the members: in terms of the block's parameters, once for
        // all the members in it.
        let mut conditions: Vec<Id<Expr>> = Vec::new();
        for &method in &block.methods {
            for &condition in &self.ast.funcs[method].when {
                if !conditions.contains(&condition) {
                    conditions.push(condition);
                }
            }
        }
        let outer = std::mem::replace(&mut self.settled_condition, true);
        for condition in conditions {
            self.resolve_expr(condition);
        }
        self.settled_condition = outer;
        self.type_params.clear();

        let self_type = self.names.define(
            som_lang::SELF_TYPE,
            DefKind::SelfType { impl_: index },
            block.target.span(),
            self.module,
        );
        let types = self.names.impls[index].types.clone();
        self.block_types = std::iter::once((som_lang::SELF_TYPE, self_type))
            .chain(
                block
                    .types
                    .iter()
                    .zip(types)
                    .map(|((n, _), d)| (&*n.name, d)),
            )
            .collect();
        let outer = std::mem::replace(&mut self.owner, self.names.impls[index].owner);
        // Its constants, named as they are inside it, `PI` as well as `Self::PI`: by each
        // other, and by its methods, whose own names come first.
        let consts = (block.consts.iter())
            .map(|&c| (&*self.ast.consts[c].name.name, self.names.const_def(c)))
            .collect();
        self.scopes.push(consts);
        for &c in &block.consts {
            let c = &self.ast.consts[c];
            if let Some(ty) = c.ty {
                self.resolve_type(ty);
            }
            self.scopes.push(HashMap::new());
            self.resolve_expr(c.value);
            self.pop_scope();
        }
        block.methods.iter().for_each(|&id| self.resolve_func(id));
        self.pop_scope();
        self.owner = outer;
        self.block_types.clear();
    }

    /// A trait's methods, which can name its associated types; `Self` is a generic of each.
    fn resolve_trait(&mut self, id: Id<som_ast::Trait>) {
        let t = &self.ast.traits[id];
        let types = self.names.trait_types(id).to_vec();
        let supers = (t.supers.iter())
            .filter_map(|path| self.resolve_path(path, Expect::Trait))
            .collect();
        self.names.set_supers(id, supers);
        // Its parameters' defaults, `Other = Self`: in terms of `Self` and its parameters.
        let this = som_ast::Ident {
            name: som_lang::SELF_TYPE.into(),
            span: t.name.span.clone(),
        };
        let params: Vec<DefId> = (std::iter::once(&this).chain(t.generics.iter().map(|g| &g.name)))
            .enumerate()
            .map(|(index, name)| {
                let kind = DefKind::TraitParam { trait_: id, index };
                self.names
                    .define(&name.name, kind, name.span.clone(), self.module)
            })
            .collect();
        self.names.set_trait_params(id, params.clone());
        self.type_params = std::iter::once(som_lang::SELF_TYPE)
            .chain(t.generics.iter().map(|g| &*g.name.name))
            .zip(params)
            .collect();
        for g in &t.generics {
            if let Some(default) = g.default {
                self.resolve_type(default);
            }
        }
        self.type_params.clear();
        self.block_types = (t.types.iter().zip(types))
            .map(|(assoc, def)| (&*assoc.name.name, def))
            .collect();
        t.methods.iter().for_each(|&m| self.resolve_func(m));
        self.block_types.clear();
    }

    /// The traits generic parameter `generic`, defined as `def`, has to implement.
    fn resolve_bounds(&mut self, generic: &'a som_ast::Generic, def: DefId) {
        let traits = (generic.bounds.iter())
            .filter_map(|path| self.resolve_path(path, Expect::Trait))
            .collect::<Vec<_>>();
        if !traits.is_empty() {
            self.names.set_bounds(def, traits);
        }
    }

    fn resolve_func(&mut self, id: Id<Func>) {
        let func = &self.ast.funcs[id];

        self.type_params.clear();
        for (index, param) in func.generics.iter().enumerate() {
            let kind = DefKind::TypeParam {
                func: id,
                index,
                value: param.ty.is_some(),
            };
            let def =
                self.names
                    .define(&param.name.name, kind, param.name.span.clone(), self.module);
            self.names.push_type_param(id, def);
            self.type_params.push((&param.name.name, def));
        }
        for (param, &def) in func
            .generics
            .iter()
            .zip(self.names.type_params(id).to_vec().iter())
        {
            if let Some(ty) = param.ty {
                self.resolve_type(ty);
            }
            self.resolve_bounds(param, def);
        }

        self.scopes.push(HashMap::new());
        for (index, param) in func.params.iter().enumerate() {
            if let Some(ty) = param.ty {
                self.resolve_type(ty);
            }
            let def = self.names.define(
                &param.name.name,
                DefKind::Param { func: id, index },
                param.name.span.clone(),
                self.module,
            );
            self.names.push_param(id, def);
            self.bind(&param.name, def);
        }
        if let Some(ty) = func.returns {
            self.resolve_type(ty);
        }
        // `@where i < self.len`: in terms of the parameters.
        for clause in &func.requires {
            self.resolve_expr(clause.condition);
        }
        // `@if N >= 3` around it, among an impl block's members: in terms of its parameters.
        if let Some(body) = func.body {
            self.resolve_expr(body);
        }
        self.pop_scope();
    }

    fn bind(&mut self, name: &'a Ident, def: DefId) {
        self.scopes
            .last_mut()
            .expect("inside a scope")
            .insert(&name.name, def);
    }

    fn in_scope(&mut self, f: impl FnOnce(&mut Self)) {
        self.scopes.push(HashMap::new());
        f(self);
        self.pop_scope();
    }

    /// Ends the innermost scope, and what its `use`s brought in.
    fn pop_scope(&mut self) {
        self.scopes.pop();
        let depth = self.scopes.len();
        self.block_imports.retain(|&(d, _)| d <= depth);
    }

    fn resolve_stmts(&mut self, stmts: &[Id<Stmt>]) {
        self.in_scope(|r| stmts.iter().for_each(|&s| r.resolve_stmt(s)));
    }

    fn resolve_stmt(&mut self, id: Id<Stmt>) {
        match &self.ast.stmts[id] {
            // A module only the rest of the block sees. One that doesn't exist was reported
            // when modules were loaded.
            Stmt::Use(u) => {
                if let Some(&module) = self.modules.get(u.path.to_string().as_str()) {
                    self.block_imports.push((self.scopes.len(), module));
                }
            }
            // Seen by the rest of the block. Its value is worked out while compiling, so it
            // can't name what's only there when the function runs.
            Stmt::Const(c) => {
                let c = *c;
                let constant = &self.ast.consts[c];
                if let Some(ty) = constant.ty {
                    self.resolve_type(ty);
                }
                let outer = self.const_depth.replace(self.scopes.len());
                self.resolve_expr(constant.value);
                self.const_depth = outer;
                let def = self.names.define(
                    &constant.name.name,
                    DefKind::Const(c),
                    constant.name.span.clone(),
                    self.module,
                );
                self.names.set_const(c, def);
                self.bind(&constant.name, def);
            }
            Stmt::Let {
                name, ty, value, ..
            } => {
                // The value can't see the binding it initialises.
                self.resolve_expr(*value);
                if let Some(ty) = ty {
                    self.resolve_type(*ty);
                }
                let def = self.names.define(
                    &name.name,
                    DefKind::Local(id),
                    name.span.clone(),
                    self.module,
                );
                self.names.set_let(id, def);
                self.bind(name, def);
            }
            // `@for f in T`, over a generic type: each struct's copy, where it's written. The
            // copies are the same text: what's wrong is said for the first.
            Stmt::Expr(_) if let Some(per_type) = self.ast.per_type.get(&id) => {
                for (k, &(_, arm)) in per_type.arms.iter().enumerate() {
                    let before = self.errors.len();
                    self.resolve_expr(arm);
                    if k > 0 {
                        self.errors.truncate(before);
                    }
                }
            }
            Stmt::Expr(expr) => self.resolve_expr(*expr),
            Stmt::Loop { body } => self.resolve_stmts(body),
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.resolve_expr(*value);
                }
            }
            Stmt::Break(target) | Stmt::Continue(target) => {
                let keyword = match self.ast.stmts[id] {
                    Stmt::Break(_) => "break",
                    _ => "continue",
                };
                if let Some(name) = target {
                    self.resolve_loop_target(id, keyword, name);
                }
            }
            // The name is only bound in the body.
            Stmt::For {
                name,
                iterable,
                body,
                ..
            } => {
                self.resolve_expr(*iterable);
                let def = self.names.define(
                    &name.name,
                    DefKind::Local(id),
                    name.span.clone(),
                    self.module,
                );
                self.names.set_let(id, def);
                self.in_scope(|r| {
                    r.bind(name, def);
                    r.resolve_stmts(body);
                });
            }
            Stmt::While { condition, body } => {
                self.resolve_expr(*condition);
                self.resolve_stmts(body);
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let settled = self.ast.settled.contains(&id);
                let outer = std::mem::replace(&mut self.settled_condition, settled);
                self.resolve_expr(*condition);
                self.settled_condition = outer;
                self.resolve_stmts(then);
                if let Some(otherwise) = otherwise {
                    self.resolve_stmts(otherwise);
                }
            }
        }
    }

    /// The `x` in `break x;` or `continue x;`: a variable in scope, which has to be an
    /// enclosing `for` loop's. An inner loop with the same name hides an outer one, as it does
    /// for reading it.
    fn resolve_loop_target(&mut self, stmt: Id<Stmt>, keyword: &'static str, name: &Ident) {
        let Some(def) = self.lookup(&name.name, Expect::Value) else {
            self.errors.push(ResolveError::UnboundVariable {
                name: name.name.to_string(),
                span: name.span.clone(),
                help: None,
                fixes: Vec::new(),
            });
            return;
        };
        let defined = self.names.def(def);
        match defined.kind {
            DefKind::Local(loop_stmt) if matches!(self.ast.stmts[loop_stmt], Stmt::For { .. }) => {
                self.names.set_loop_target(stmt, def);
            }
            _ => self.errors.push(ResolveError::NotALoop {
                keyword,
                name: name.name.to_string(),
                span: name.span.clone(),
                defined_at: defined.span.clone(),
            }),
        }
    }

    fn resolve_expr(&mut self, id: Id<Expr>) {
        match &self.ast.exprs[id] {
            Expr::Fail(message) => self.resolve_expr(*message),
            // Worked out while compiling: it can't name what's only there when the program runs.
            Expr::InPlace(c) => {
                let c = *c;
                let constant = &self.ast.consts[c];
                let outer = self.const_depth.replace(self.scopes.len());
                self.resolve_expr(constant.value);
                self.const_depth = outer;
                let def = self.names.define(
                    &constant.name.name,
                    DefKind::Const(c),
                    constant.name.span.clone(),
                    self.module,
                );
                self.names.set_const(c, def);
            }
            Expr::Hole
            | Expr::Unreachable
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None => {}
            Expr::Type(ty) => self.resolve_type(*ty),
            // `@if T == Point`: a type compared with another, where nothing else is named so.
            Expr::Binary {
                op: som_ast::BinOp::Equals | som_ast::BinOp::NotEquals,
                lhs,
                rhs,
            } if self.settled_condition => {
                for operand in [*lhs, *rhs] {
                    if let Expr::Var(path) = &self.ast.exprs[operand]
                        && let [only] = &path.segments[..]
                        && self.lookup(&only.name, Expect::Value).is_none()
                        && let Some(def) = self.lookup(&only.name, Expect::Type)
                    {
                        self.names.set_expr(operand, def);
                    } else {
                        self.resolve_expr(operand);
                    }
                }
            }
            // `usize::MAX`: an integer type's limit, which isn't defined anywhere.
            Expr::Var(path) if path.int_limit().is_some() => {}
            Expr::Var(path) => self.bind_expr(id, path, Expect::Value),
            // Visible from here to the end of the block, like a `let` before the call.
            Expr::Out(name) => {
                let def =
                    self.names
                        .define(&name.name, DefKind::Out(id), name.span.clone(), self.module);
                self.names.set_expr(id, def);
                self.bind(name, def);
            }
            Expr::Binary { lhs, rhs, .. }
            | Expr::Fallback {
                optional: lhs,
                fallback: rhs,
            } => {
                self.resolve_expr(*lhs);
                self.resolve_expr(*rhs);
            }
            Expr::Not(inner)
            | Expr::Neg(inner)
            | Expr::BitNot(inner)
            | Expr::Deref(inner)
            | Expr::Borrow { expr: inner, .. } => self.resolve_expr(*inner),
            Expr::ArrayRepeat { value, count } => {
                self.resolve_expr(*value);
                self.resolve_type(*count);
            }
            Expr::Array(items) => items.iter().for_each(|&item| self.resolve_expr(item)),
            Expr::Interpolated(parts) => {
                for part in parts {
                    if let som_ast::Part::Value(value) = part {
                        self.resolve_expr(*value);
                    }
                }
            }
            Expr::Range { start, end } => {
                start.iter().chain(end).for_each(|&e| self.resolve_expr(e));
            }
            Expr::Cast { value, ty } => {
                self.resolve_expr(*value);
                self.resolve_type(*ty);
            }
            Expr::Assign { target, value } => {
                self.resolve_expr(*target);
                self.resolve_expr(*value);
            }
            Expr::Call {
                callee,
                owner_args,
                type_args,
                args,
            } => {
                // `range(n)`, `puts(x)`: another language's, where nothing here's called that.
                if self.foreign_call(id, callee, args) {
                    args.iter().for_each(|&a| self.resolve_expr(a));
                    return;
                }
                self.bind_expr(id, callee, Expect::Func);
                (owner_args.iter().chain(type_args)).for_each(|&t| self.resolve_type(t));
                args.iter().for_each(|&a| self.resolve_expr(a));
            }
            // The method itself depends on the receiver's type, so the type checker finds it.
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => {
                // `console.log(x)`: another language's way of printing.
                if self.foreign_print(id, *receiver, method) {
                    args.iter().for_each(|&a| self.resolve_expr(a));
                    return;
                }
                self.resolve_expr(*receiver);
                args.iter().for_each(|&a| self.resolve_expr(a));
            }
            Expr::Field { object, .. } => self.resolve_expr(*object),
            Expr::Index { object, index } => {
                self.resolve_expr(*object);
                self.resolve_expr(*index);
            }
            Expr::Construct { ty, args, fields } => {
                self.bind_expr(id, ty, Expect::Struct);
                args.iter().for_each(|&arg| self.resolve_type(arg));
                fields
                    .iter()
                    .for_each(|(_, value)| self.resolve_expr(*value));
            }
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                self.resolve_expr(*condition);
                self.resolve_expr(*then);
                self.resolve_expr(*otherwise);
            }
            Expr::Block { stmts, value } => self.in_scope(|r| {
                stmts.iter().for_each(|&s| r.resolve_stmt(s));
                if let Some(value) = value {
                    r.resolve_expr(*value);
                }
            }),
            // `@for (i, k) in T` among its arms, over a generic enum: each enum's copy. The
            // copies are the same text: what's wrong is said for the first.
            Expr::Match { scrutinee, .. }
                if let Some(per_type) = self.ast.per_type_matches.get(&id) =>
            {
                self.resolve_expr(*scrutinee);
                for (k, &(_, copy)) in per_type.arms.iter().enumerate() {
                    let before = self.errors.len();
                    self.resolve_expr(copy);
                    if k > 0 {
                        self.errors.truncate(before);
                    }
                }
            }
            // What an arm's pattern binds is only seen in that arm.
            Expr::Match { scrutinee, arms } => {
                self.resolve_expr(*scrutinee);
                for arm in arms {
                    self.in_scope(|r| {
                        r.resolve_pattern(arm.pattern);
                        r.resolve_expr(arm.body);
                    });
                }
            }
        }
    }

    fn resolve_pattern(&mut self, id: Id<Pattern>) {
        match &self.ast.patterns[id] {
            Pattern::Wildcard
            | Pattern::Int(_)
            | Pattern::Bool(_)
            | Pattern::Str(_)
            | Pattern::None => {}
            // `Empty`, if some enum in scope has a variant of that name: the type matched
            // decides whose.
            Pattern::Binding { name, mutable } => {
                if !*mutable && self.is_variant_name(&name.name) {
                    return;
                }
                let def = (self.names).define(
                    &name.name,
                    DefKind::Binding(id),
                    name.span.clone(),
                    self.module,
                );
                self.names.set_pattern(id, def);
                self.bind(name, def);
            }
            Pattern::Variant { path, fields } => {
                // `Shape::Circle(r)`; a bare `Circle(r)` is found from the type matched.
                if path.segments.len() > 1
                    && let Some(def) = self.resolve_path(path, Expect::Value)
                {
                    self.names.set_pattern(id, def);
                }
                match fields {
                    PatternFields::Unit => {}
                    PatternFields::Tuple(fields) => {
                        fields.iter().for_each(|&f| self.resolve_pattern(f));
                    }
                    PatternFields::Named { fields, .. } => {
                        fields.iter().for_each(|&(_, f)| self.resolve_pattern(f));
                    }
                }
            }
        }
    }

    /// Whether `name` is a variant of an enum in the current module, one it uses, or the
    /// prelude: `Less` of `Ordering`.
    fn is_variant_name(&self, name: &str) -> bool {
        let prelude = self.modules.get(som_lang::PRELUDE_MODULE).copied();
        std::iter::once(self.module)
            .chain(self.block_imports.iter().rev().map(|&(_, m)| m))
            .chain(self.imports[self.module].iter().copied())
            .chain(prelude)
            .any(|m| self.variant_names[m].contains(name))
    }

    fn bind_expr(&mut self, id: Id<Expr>, path: &Path, expect: Expect) {
        self.through_alias.set(None);
        if let Some(def) = self.resolve_path(path, expect) {
            // A local from outside the constant. A generic parameter, `N`, is known: it's
            // worked out once for each set of arguments the function is used with.
            let outside = |r: &Self| {
                let depth = r.const_depth.unwrap_or(0);
                (r.scopes
                    .iter()
                    .rposition(|scope| scope.values().any(|&d| d == def)))
                .is_some_and(|at| at < depth)
            };
            let local = matches!(
                self.names.def(def).kind,
                DefKind::Local(_) | DefKind::Param { .. } | DefKind::Out(_) | DefKind::Binding(_)
            );
            if self.const_depth.is_some() && local && outside(self) {
                self.errors.push(ResolveError::ConstNotKnown {
                    span: self.ast.exprs.span(id).clone(),
                });
            }
            self.names.set_expr(id, def);
            if let Some(alias) = self.through_alias.take() {
                self.names.set_expr_alias(id, alias);
            }
        }
    }

    /// `def` if it's something `expect` accepts; or, where a struct is expected, the struct
    /// alias `def` stands for, noting the alias in `through_alias`.
    fn fitting(&self, def: DefId, expect: Expect) -> Option<DefId> {
        if self.fits(def, expect) {
            return Some(def);
        }
        if expect != Expect::Struct {
            return None;
        }
        let strukt = self.aliased_struct(def)?;
        self.through_alias.set(Some(def));
        Some(strukt)
    }

    /// The struct alias `def` stands for, through any aliases it's written with.
    fn aliased_struct(&self, mut def: DefId) -> Option<DefId> {
        // An alias that refers to itself stands for nothing; the type checker says so.
        for _ in 0..=self.names.aliases().len() {
            match self.names.def(def).kind {
                DefKind::Alias { ty } => def = self.names.ty(ty)?,
                DefKind::Struct(_) => return Some(def),
                _ => return None,
            }
        }
        None
    }

    fn resolve_type(&mut self, id: Id<TypeExpr>) {
        match &self.ast.types[id] {
            // `T::Item`, `Self::Item`: an associated type of a type parameter's trait.
            TypeExpr::Named { path, args }
                if path.segments.len() == 2
                    && let Some(base) = self.lookup(&path.segments[0].name, Expect::Type)
                    && let Some(assoc) = self.associated(base, &path.segments[1].name) =>
            {
                self.names.set_type(id, assoc);
                if !matches!(self.names.def(assoc).kind, DefKind::ImplType { .. }) {
                    self.names.set_projection_base(id, base);
                }
                args.iter().for_each(|&arg| self.resolve_type(arg));
            }
            TypeExpr::Named { path, args } => {
                if let Some(def) = self.resolve_path(path, Expect::Type) {
                    self.names.set_type(id, def);
                }
                args.iter().for_each(|&arg| self.resolve_type(arg));
            }
            TypeExpr::Reference { to, .. }
            | TypeExpr::PointerTo(to)
            | TypeExpr::Optional(to)
            | TypeExpr::Shared(to)
            | TypeExpr::Elements(to)
            | TypeExpr::Slice { element: to, .. } => self.resolve_type(*to),
            TypeExpr::Array {
                element: lhs,
                len: rhs,
            }
            | TypeExpr::Binary { lhs, rhs, .. } => {
                self.resolve_type(*lhs);
                self.resolve_type(*rhs);
            }
            TypeExpr::Fn { params, returns } => {
                params.iter().for_each(|&p| self.resolve_type(p));
                if let Some(returns) = returns {
                    self.resolve_type(*returns);
                }
            }
            TypeExpr::Unit
            | TypeExpr::Primitive(_)
            | TypeExpr::Pointer
            | TypeExpr::Int(_)
            | TypeExpr::Bool(_) => {}
        }
    }

    /// The associated type `name` of what `base` names: of a trait a type parameter has to
    /// implement, or the one an impl block says, for its `Self`.
    fn associated(&self, base: DefId, name: &str) -> Option<DefId> {
        let named = |defs: &[DefId]| {
            defs.iter()
                .copied()
                .find(|&d| &*self.names.def(d).name == name)
        };
        match self.names.def(base).kind {
            DefKind::SelfType { impl_ } => named(&self.names.impls[impl_].types),
            DefKind::TypeParam { .. } | DefKind::StructParam { .. } | DefKind::ImplParam { .. } => {
                self.names
                    .bounds(base)
                    .iter()
                    .find_map(|&t| match self.names.def(t).kind {
                        DefKind::Trait(t) => named(self.names.trait_types(t)),
                        _ => None,
                    })
            }
            _ => None,
        }
    }

    // --- Lookup ---

    /// Resolves `path`, reporting an error and returning `None` if it doesn't name an `expect`.
    fn resolve_path(&mut self, path: &Path, expect: Expect) -> Option<DefId> {
        // `c::puts` without `extern "c"`: what a library has is there once it's declared.
        if let Some(library) = self.undeclared_library(path) {
            let span = path.span();
            let line = format!("extern \"{library}\";");
            self.errors.push(ResolveError::LibraryNotDeclared {
                library: library.to_string(),
                fix: Fix::add_line(msg!["add ", code(line.clone())], span.source.clone(), &line),
                span,
            });
            return None;
        }
        let found = if path.segments.len() == 1 {
            self.lookup(&path.segments[0].name, expect)
        } else {
            self.lookup_qualified(path, expect)
        };
        if found.is_none() {
            let error = self.not_found(path, expect);
            self.errors.push(error);
        }
        // Found, but not to be seen from here: `pub` or `mod` would let it be.
        if let Some(def) = found {
            let span = path.span();
            let viewer = visibility::Viewer {
                module: self.module,
                file: span.source.identifier(),
                owner: self.owner,
            };
            if let Some(hidden) = visibility::hidden(self.ast, &self.names, def, viewer) {
                self.errors.push(ResolveError::NotVisible {
                    name: path.to_string(),
                    span: span.clone(),
                    hidden,
                });
            }
        }
        found
    }

    fn lookup(&self, name: &str, expect: Expect) -> Option<DefId> {
        if expect == Expect::Value
            && let Some(&def) = self.scopes.iter().rev().find_map(|scope| scope.get(name))
        {
            return Some(def);
        }
        // `Self` and associated types inside a trait or impl block.
        if expect == Expect::Type
            && let Some(&(_, def)) = self.block_types.iter().find(|(n, _)| *n == name)
        {
            return Some(def);
        }
        // `Self { … }` and `Self::new()` in an impl block: the struct or built-in type it's for.
        if expect == Expect::Struct
            && name == som_lang::SELF_TYPE
            && let Some(&(_, def)) = self.block_types.iter().find(|(n, _)| *n == name)
            && let DefKind::SelfType { impl_ } = self.names.def(def).kind
        {
            return self.names.impls[impl_].owner;
        }
        // A value parameter is a value too: `N` in `struct Buffer<N: usize>`.
        if let Some(&(_, def)) = self.type_params.iter().find(|(n, _)| *n == name)
            && (expect == Expect::Type
                || (expect == Expect::Value && self.names.is_value_param(def)))
        {
            return Some(def);
        }

        // The standard library's root module, `som`, is seen from everywhere: the traits the
        // language uses are there.
        let prelude = self.modules.get(som_lang::PRELUDE_MODULE).copied();
        std::iter::once(self.module)
            .chain(self.block_imports.iter().rev().map(|&(_, m)| m))
            .chain(self.imports[self.module].iter().copied())
            .chain(prelude)
            .find_map(|m| self.items[m].get(name).copied())
            .and_then(|def| self.fitting(def, expect))
    }

    /// The library `path` names something of, `c` in `c::puts`, if this module has no
    /// `extern` for it.
    fn undeclared_library(&self, path: &Path) -> Option<&'a str> {
        let segments = &path.segments;
        let module = (1..segments.len()).rev().find_map(|len| {
            let name: Vec<&str> = segments[..len].iter().map(|s| &*s.name).collect();
            self.modules.get(name.join("::").as_str()).copied()
        })?;
        let library = self.libraries.get(&module)?;
        (!self.declared[self.module].contains(&module)).then_some(*library)
    }

    /// `module::item`, `Struct::method`, or `module::Struct::method`.
    fn lookup_qualified(&self, path: &Path, expect: Expect) -> Option<DefId> {
        let segments = &path.segments;

        // The longest prefix that names a module, if any. Module names can contain `::`.
        // A module this one `use`s is named by its last part too: `io::println` after
        // `use som::io;`.
        let module_prefix = (1..segments.len())
            .rev()
            .find_map(|len| {
                let name = segments[..len]
                    .iter()
                    .map(|s| &*s.name)
                    .collect::<Vec<_>>()
                    .join("::");
                self.modules.get(name.as_str()).map(|&m| (m, len))
            })
            .or_else(|| {
                let first = &*segments[0].name;
                (self.block_imports.iter().rev().map(|&(_, m)| m))
                    .chain(self.imports[self.module].iter().copied())
                    .find(|&m| self.ast.modules[m].name.rsplit("::").next() == Some(first))
                    .map(|m| (m, 1))
            });

        let (owner, rest) = match module_prefix {
            Some((m, len)) => {
                let rest = &segments[len..];
                if rest.len() == 1 {
                    return self.items[m]
                        .get(&*rest[0].name)
                        .copied()
                        .and_then(|d| self.fitting(d, expect));
                }
                let owner = self.items[m].get(&*rest[0].name).copied()?;
                (
                    self.fitting(owner, Expect::Struct).unwrap_or(owner),
                    &rest[1..],
                )
            }
            None => (
                self.lookup(&segments[0].name, Expect::Struct)
                    .or_else(|| self.primitive_named(&segments[0].name))
                    .or_else(|| self.lookup(&segments[0].name, Expect::Trait))?,
                &segments[1..],
            ),
        };

        match rest {
            [name] if matches!(self.names.def(owner).kind, DefKind::Primitive(_)) => self
                .names
                .method(owner, &name.name)
                .or_else(|| self.names.assoc_const(owner, &name.name))
                .filter(|&d| self.fits(d, expect)),
            [name] if matches!(self.names.def(owner).kind, DefKind::Struct(_)) => (self.names)
                .variant(owner, &name.name)
                .or_else(|| self.names.method(owner, &name.name))
                .or_else(|| self.names.assoc_const(owner, &name.name))
                .filter(|&d| self.fits(d, expect)),
            // `Hash::hash(&value, &mut hasher)`: a trait's method, for whichever type `self`
            // turns out to be.
            [name] => match self.names.def(owner).kind {
                DefKind::Trait(t) => (self.ast.traits[t].methods.iter())
                    .find(|&&m| self.ast.funcs[m].name.name == name.name)
                    .map(|&m| self.names.func(m))
                    .filter(|&d| self.fits(d, expect)),
                _ => None,
            },
            _ => None,
        }
    }

    /// The definition of the built-in type called `name`, if it has methods of its own.
    fn primitive_named(&self, name: &str) -> Option<DefId> {
        self.names.primitives_named(name)
    }

    /// Whether `def` is something `expect` accepts.
    fn fits(&self, def: DefId, expect: Expect) -> bool {
        let kind = &self.names.def(def).kind;
        match expect {
            Expect::Value => !matches!(
                kind,
                DefKind::Struct(_)
                    | DefKind::TypeParam { .. }
                    | DefKind::TraitParam { .. }
                    | DefKind::StructParam { .. }
                    | DefKind::Trait(_)
                    | DefKind::AssocType { .. }
                    | DefKind::ImplParam { .. }
                    | DefKind::SelfType { .. }
                    | DefKind::ImplType { .. }
                    | DefKind::Alias { .. }
                    | DefKind::AliasParam { .. }
            ),
            // `Shape::Circle(1.0)` and `Shape::Rect { … }` make a variant.
            Expect::Func => matches!(kind, DefKind::Func(_) | DefKind::Variant { .. }),
            Expect::Struct => matches!(kind, DefKind::Struct(_) | DefKind::Variant { .. }),
            Expect::Type => matches!(
                kind,
                DefKind::Struct(_)
                    | DefKind::TypeParam { .. }
                    | DefKind::TraitParam { .. }
                    | DefKind::StructParam { .. }
                    | DefKind::AssocType { .. }
                    | DefKind::ImplParam { .. }
                    | DefKind::SelfType { .. }
                    | DefKind::ImplType { .. }
                    | DefKind::Alias { .. }
                    | DefKind::AliasParam { .. }
                    // An array's length: `[i32; W]`.
                    | DefKind::Const(_)
            ),
            Expect::Trait => matches!(kind, DefKind::Trait(_)),
        }
    }

    // --- Errors ---

    fn not_found(&self, path: &Path, expect: Expect) -> ResolveError {
        let name = path.to_string();
        let span = path.span();
        let (help, fixes) = self.suggest(path, expect).unwrap_or_default();
        let help = Some(help).filter(|h| !h.is_empty());
        match expect {
            Expect::Value => ResolveError::UnboundVariable {
                name,
                span,
                help,
                fixes,
            },
            Expect::Func => ResolveError::UnknownFunction {
                name,
                span,
                help,
                fixes,
            },
            Expect::Struct => ResolveError::UnknownStruct {
                name,
                span,
                help,
                fixes,
            },
            Expect::Type => ResolveError::UnknownType {
                name,
                span,
                help,
                fixes,
            },
            Expect::Trait => ResolveError::UnknownTrait {
                name,
                span,
                help,
                fixes,
            },
        }
    }

    /// A hint for a name that wasn't found, and the changes it suggests.
    fn suggest(&self, path: &Path, expect: Expect) -> Option<(Message, Vec<Fix>)> {
        let span = path.span();
        let change_to = |text: String| vec![Fix::change_to(span.clone(), text)];
        let name = &*path.name().name;

        if path.segments.len() > 1 {
            // Look among what the qualifier names: a module's items, or a struct's methods.
            let qualifier: Vec<&str> = path.qualifier().iter().map(|s| &*s.name).collect();
            let qualifier = qualifier.join("::");
            let candidates: Vec<String> = if let Some(&m) = self.modules.get(qualifier.as_str()) {
                self.items[m]
                    .iter()
                    .filter(|&(_, &d)| self.fits(d, expect))
                    .map(|(n, _)| n.to_string())
                    .collect()
            } else if let Some(owner) = self.lookup(&path.segments[0].name, Expect::Struct)
                && path.segments.len() == 2
            {
                let variants = match self.names.def(owner).kind {
                    DefKind::Struct(s) => self.names.variants(s).to_vec(),
                    _ => Vec::new(),
                };
                (variants.iter())
                    .map(|&v| self.names.def(v).name.to_string())
                    .chain(
                        self.names
                            .methods_of(owner)
                            .into_iter()
                            .map(|(n, _)| n.to_string()),
                    )
                    .collect()
            } else {
                return None;
            };
            if let Some(m) = best_match(name, &candidates) {
                let text = format!("{qualifier}::{m}");
                return Some((msg!["did you mean ", code(&text), "?"], change_to(text)));
            }
            return self.constructors_hint(path, &qualifier, expect);
        }

        if expect == Expect::Value
            && let Some(hint) = self.other_language_hint(name, &change_to)
        {
            return Some(hint);
        }
        if expect == Expect::Type
            && let Some(hint) = self.foreign_type_hint(name, &change_to)
        {
            return Some(hint);
        }

        // A close match among the names visible here?
        let mut visible: Vec<String> = std::iter::once(self.module)
            .chain(self.imports[self.module].iter().copied())
            .flat_map(|m| self.items[m].iter())
            .filter(|&(_, &def)| self.fits(def, expect))
            .map(|(name, _)| name.to_string())
            .collect();
        if expect == Expect::Value {
            visible.extend(
                self.scopes
                    .iter()
                    .flat_map(|s| s.keys())
                    .map(|n| n.to_string()),
            );
        }
        visible.sort();
        visible.dedup();
        if let Some(m) = best_match(name, &visible) {
            return Some((
                msg!["did you mean ", expect.name(m), "?"],
                change_to(m.to_string()),
            ));
        }

        // Defined in a loaded module that isn't in scope here, maybe spelled in snake_case?
        let snake = snake_case(name);
        for (index, (module, items)) in self.ast.modules.iter().zip(&self.items).enumerate() {
            if module.name.is_empty()
                || index == self.module
                || self.imports[self.module].contains(&index)
            {
                continue;
            }
            let Some(found) = [name, snake.as_str()]
                .into_iter()
                .find(|n| items.get(n).is_some_and(|&d| self.fits(d, expect)))
            else {
                continue;
            };
            let m = &module.name;
            let (qualified, import) = (format!("{m}::{found}"), format!("use {m};"));
            let help = msg![
                expect.name(found),
                " is defined in module ",
                message::module(m),
                " but is not accessible from here. try ",
                code(&qualified),
                " or ",
                code(&import),
            ];
            let mut fixes = change_to(qualified);
            fixes.push(Fix::add_line(
                msg!["add ", code(&import)],
                span.source.clone(),
                &import,
            ));
            return Some((help, fixes));
        }

        None
    }
}

impl Resolver<'_> {
    /// For `List::new` where `List` has no `new`: the functions that make one, those not
    /// taking `self`. Only for names other languages give constructors.
    fn constructors_hint(
        &self,
        path: &Path,
        qualifier: &str,
        expect: Expect,
    ) -> Option<(Message, Vec<Fix>)> {
        let name = &*path.name().name;
        if expect != Expect::Func
            || path.segments.len() != 2
            || !foreign::CONSTRUCTOR.contains(&name)
        {
            return None;
        }
        let owner = self.lookup(&path.segments[0].name, Expect::Struct)?;
        // Its functions not taking `self` (however it's taken: `self`, `&self`, `&mut self`),
        // with whether they take nothing at all.
        let constructors: Vec<(String, bool)> = (self.names.methods_of(owner).into_iter())
            .filter_map(|(n, def)| match self.names.def(def).kind {
                DefKind::Func(f) => {
                    let func = &self.ast.funcs[f];
                    let takes_self = func
                        .params
                        .first()
                        .is_some_and(|p| &*p.name.name == som_lang::SELF_VALUE);
                    (!func.prop && !takes_self)
                        .then(|| (format!("{qualifier}::{n}"), func.params.is_empty()))
                }
                _ => None,
            })
            .collect();
        if constructors.is_empty() {
            return None;
        }
        let mut help = msg![
            code(qualifier),
            " has no ",
            code(name),
            "; to make one, use "
        ];
        for (i, (c, _)) in constructors.iter().enumerate() {
            if i > 0 {
                help.push(if i + 1 == constructors.len() {
                    " or "
                } else {
                    ", "
                });
            }
            help.push(code(c));
        }
        // Only those taking nothing can stand in as they are: `List::new()` → `List::empty()`.
        let fixes = (constructors.iter())
            .filter(|(_, nothing)| *nothing)
            .map(|(c, _)| Fix::change_to(path.span(), c.clone()))
            .collect();
        Some((help, fixes))
    }

    /// A value named as another language names it: `null` for `none`, `this` for `self`.
    fn other_language_hint(
        &self,
        name: &str,
        change_to: &dyn Fn(String) -> Vec<Fix>,
    ) -> Option<(Message, Vec<Fix>)> {
        let has_self = self
            .scopes
            .iter()
            .any(|s| s.contains_key(som_lang::SELF_VALUE));
        let found =
            foreign::value(name).or_else(|| foreign::self_word(name).filter(|_| has_self))?;
        let help = msg!["som writes ", code(found.som), ": ", found.hint()];
        Some((help, change_to(found.som.to_string())))
    }

    /// `range(n)` or `range(a, b)`, Python's count, or `puts(x)`, Ruby's print, where
    /// nothing's called that: reported, with som's way. Whether it was.
    fn foreign_call(&mut self, id: Id<Expr>, callee: &Path, args: &[Id<Expr>]) -> bool {
        let [segment] = callee.segments.as_slice() else {
            return false;
        };
        let name = &*segment.name;
        if self.lookup(name, Expect::Func).is_some() {
            return false;
        }
        let span = self.ast.exprs.span(id).clone();
        let text = |e: Id<Expr>| self.ast.exprs.span(e).get_text().to_string();
        let (help, fixes) = if let Some(found) = foreign::len_function(name) {
            // `len(xs)`: `xs.len`, a property.
            let [of] = args else {
                return false;
            };
            let read = format!("{}.len", text(*of));
            let help = msg!["som reads a length as ", code(&read), ": ", found.hint()];
            let fix = Fix::change_to(span, read);
            (help, vec![fix])
        } else if let Some(found) = foreign::range(name) {
            let range = match args {
                [end] => format!("0..{}", text(*end)),
                [start, end] => format!("{}..{}", text(*start), text(*end)),
                _ => return false,
            };
            let help = msg!["som writes a range ", code(&range), ": ", found.hint()];
            let fix = Fix::change_to(span, range);
            (help, vec![fix])
        } else if let Some(found) = foreign::print(name) {
            let help = msg![
                "som prints a line with ",
                code("println(…)"),
                ": ",
                found.hint()
            ];
            let fix = Fix::change_to(segment.span.clone(), "println");
            (help, vec![fix])
        } else {
            return false;
        };
        self.errors.push(ResolveError::UnknownFunction {
            name: name.to_string(),
            span: segment.span.clone(),
            help: Some(help),
            fixes,
        });
        true
    }

    /// `console.log(x)`, `System.out.println(x)`: printing as another language does, where
    /// nothing's called its first name. Reported, with som's `println`. Whether it was.
    fn foreign_print(&mut self, id: Id<Expr>, receiver: Id<Expr>, method: &Ident) -> bool {
        // The names called through, first to last: `System`, `out`.
        let mut names = vec![&*method.name];
        let mut at = receiver;
        let root = loop {
            match &self.ast.exprs[at] {
                Expr::Field { object, field } => {
                    names.push(&field.name);
                    at = *object;
                }
                Expr::Var(path) if path.segments.len() == 1 => break &path.segments[0],
                _ => return false,
            }
        };
        names.push(&root.name);
        names.reverse();
        let path = names.join(".");
        let Some(found) = foreign::print(&path) else {
            return false;
        };
        if self.lookup(&root.name, Expect::Value).is_some() {
            return false;
        }
        let span = self.ast.exprs.span(receiver).merge(&method.span);
        let help = msg![
            "som prints a line with ",
            code("println(…)"),
            ": ",
            found.hint()
        ];
        let fix = Fix::change_to(span.clone(), "println");
        let _ = id;
        self.errors.push(ResolveError::UnknownFunction {
            name: path,
            span,
            help: Some(help),
            fixes: vec![fix],
        });
        true
    }

    /// A type named as another language names it: `int` for `i32`.
    fn foreign_type_hint(
        &self,
        name: &str,
        change_to: &dyn Fn(String) -> Vec<Fix>,
    ) -> Option<(Message, Vec<Fix>)> {
        let found = foreign::type_name(name)?;
        // `float`: `f32` or `f64`, depending on where it's from.
        if found.som.is_empty() {
            let help = msg![
                "som's floats are ",
                code("f32"),
                " and ",
                code("f64"),
                ": ",
                found.hint()
            ];
            return Some((help, Vec::new()));
        }
        let help = msg!["som writes ", code(found.som), ": ", found.hint()];
        Some((help, change_to(found.som.to_string())))
    }
}

/// The best suggestion for a misspelled `name`: its snake_case spelling if that exists
/// (`InitWindow` for `init_window`, as imported C functions are named), otherwise the
/// closest name.
fn best_match<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
    let snake = snake_case(name);
    candidates
        .iter()
        .find(|c| snake != name && **c == snake)
        .map(String::as_str)
        .or_else(|| closest_match(name, candidates, 2))
}

/// The constants `expr` names, directly, through the parts a plain constant is made of.
fn consts_in(ast: &Ast, names: &Names, expr: Id<Expr>) -> Vec<Id<som_ast::Const>> {
    let mut found = Vec::new();
    let mut todo = vec![expr];
    while let Some(expr) = todo.pop() {
        if let Expr::Var(_) = ast.exprs[expr]
            && let Some(def) = names.expr(expr)
            && let DefKind::Const(c) = names.def(def).kind
        {
            found.push(c);
        }
        todo.extend(const_parts(ast, expr));
    }
    found
}

/// The expressions a plain constant's value is made of: what's inside `expr`.
fn const_parts(ast: &Ast, expr: Id<Expr>) -> Vec<Id<Expr>> {
    match &ast.exprs[expr] {
        Expr::Array(items) => items.clone(),
        Expr::Cast { value, .. }
        | Expr::ArrayRepeat { value, .. }
        | Expr::Not(value)
        | Expr::Neg(value)
        | Expr::BitNot(value) => vec![*value],
        Expr::Binary { lhs, rhs, .. } => vec![*lhs, *rhs],
        Expr::Construct { fields, .. } => fields.iter().map(|(_, v)| *v).collect(),
        Expr::Conditional {
            condition,
            then,
            otherwise,
        } => vec![*condition, *then, *otherwise],
        _ => Vec::new(),
    }
}

/// Whether constant `c`'s value runs code, a call, so it's worked out by running it while
/// compiling. A plain one, made of literals, operators, casts, other plain constants, and
/// arrays and structs of those, is written out where it's used instead.
pub fn const_runs_code(ast: &Ast, names: &Names, c: Id<som_ast::Const>) -> bool {
    fn runs_code(
        ast: &Ast,
        names: &Names,
        expr: Id<Expr>,
        seen: &mut HashSet<Id<som_ast::Const>>,
    ) -> bool {
        let plain = match &ast.exprs[expr] {
            Expr::Hole
            | Expr::Type(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None
            | Expr::Array(_)
            | Expr::Cast { .. }
            | Expr::ArrayRepeat { .. }
            | Expr::Not(_)
            | Expr::Neg(_)
            | Expr::BitNot(_)
            | Expr::Binary { .. }
            | Expr::Construct { .. }
            | Expr::Conditional { .. } => true,
            // Another constant, `usize::MAX`, or a variant without values.
            Expr::Var(path) => match names.expr(expr).map(|def| &names.def(def).kind) {
                Some(&DefKind::Const(other)) => {
                    !seen.insert(other) || !runs_code(ast, names, ast.consts[other].value, seen)
                }
                Some(DefKind::Variant { .. }) | None => true,
                Some(_) => path.int_limit().is_some(),
            },
            _ => false,
        };
        !plain || (const_parts(ast, expr).into_iter()).any(|part| runs_code(ast, names, part, seen))
    }
    let mut seen = HashSet::from([c]);
    runs_code(ast, names, ast.consts[c].value, &mut seen)
}
