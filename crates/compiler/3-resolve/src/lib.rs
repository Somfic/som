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

pub use error::ResolveError;
pub use names::{Def, DefId, DefKind, ImplDef, LangTrait, Names};

use som_ast::{Ast, Expr, Func, Id, Ident, Item, Path, Pattern, PatternFields, Stmt, TypeExpr};
use som_base::diagnostics::{closest_match, snake_case};
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
        variant_names: vec![HashSet::new(); ast.modules.len()],
        module: 0,
        scopes: Vec::new(),
        type_params: Vec::new(),
        block_types: Vec::new(),
    };
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
    names: Names,
    errors: Vec<ResolveError>,
    /// Module name → index into `ast.modules`.
    modules: HashMap<&'a str, usize>,
    /// Per module: its functions and structs, by name.
    items: Vec<HashMap<&'a str, DefId>>,
    /// Per module: the modules it `use`s, in order.
    imports: Vec<Vec<usize>>,
    /// Per module: the names of its enums' variants. A bare name in a pattern that's one of
    /// those, here or in a module this one uses, is that variant rather than a new binding.
    variant_names: Vec<HashSet<&'a str>>,
    /// The module whose code is being resolved.
    module: usize,
    /// Local scopes of the function being resolved, innermost last.
    scopes: Vec<HashMap<&'a str, DefId>>,
    /// Type parameters of the function being resolved.
    type_params: Vec<(&'a str, DefId)>,
    /// Types named inside a trait or impl block besides its parameters: `Self`, and its
    /// associated types, `Item`.
    block_types: Vec<(&'a str, DefId)>,
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
                    Item::Use(_) => {}
                }
            }
        }
        // The traits the language uses, from the standard library's root module.
        if let Some(&root) = self.modules.get("som") {
            for lang in LangTrait::ALL {
                if let Some(&def) = self.items[root].get(lang.name())
                    && matches!(self.names.def(def).kind, DefKind::Trait(_))
                {
                    self.names.set_lang(lang, def);
                }
            }
        }
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
                let owner = self.resolve_path(&block.target, Expect::Struct);
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
                let Some(owner) = owner else {
                    continue;
                };
                for &method in &block.methods {
                    let name = &ast.funcs[method].name;
                    let def = self.names.func(method);
                    // Methods can share a name, like functions: `get` taking `&self` and
                    // `&mut self`. The type checker picks the one that fits.
                    if let Some(previous) = self.names.add_method(owner, &name.name, def) {
                        self.names.add_overload(previous, def);
                    }
                }
            }
        }
    }

    // --- Phase 2: bodies ---

    fn resolve_bodies(&mut self) {
        let ast = self.ast;
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
                    Item::Use(_) => {}
                }
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
            .chain(block.trait_.iter().flat_map(|(_, args)| args))
            .chain(block.types.iter().map(|(_, ty)| ty));
        for &ty in written {
            self.resolve_type(ty);
        }
        self.type_params.clear();

        let self_type = self.names.define(
            "Self",
            DefKind::SelfType { impl_: index },
            block.target.span(),
            self.module,
        );
        let types = self.names.impls[index].types.clone();
        self.block_types = std::iter::once(("Self", self_type))
            .chain(
                block
                    .types
                    .iter()
                    .zip(types)
                    .map(|((n, _), d)| (&*n.name, d)),
            )
            .collect();
        block.methods.iter().for_each(|&id| self.resolve_func(id));
        self.block_types.clear();
    }

    /// A trait's methods, which can name its associated types; `Self` is a generic of each.
    fn resolve_trait(&mut self, id: Id<som_ast::Trait>) {
        let t = &self.ast.traits[id];
        let types = self.names.trait_types(id).to_vec();
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
        if let Some(body) = func.body {
            self.resolve_expr(body);
        }
        self.scopes.pop();
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
        self.scopes.pop();
    }

    fn resolve_stmts(&mut self, stmts: &[Id<Stmt>]) {
        self.in_scope(|r| stmts.iter().for_each(|&s| r.resolve_stmt(s)));
    }

    fn resolve_stmt(&mut self, id: Id<Stmt>) {
        match &self.ast.stmts[id] {
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
                self.resolve_expr(*condition);
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
            Expr::Hole
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None => {}
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
            Expr::Not(inner) | Expr::Deref(inner) | Expr::Borrow { expr: inner, .. } => {
                self.resolve_expr(*inner)
            }
            Expr::ArrayRepeat { value, count } => {
                self.resolve_expr(*value);
                self.resolve_type(*count);
            }
            Expr::Array(items) => items.iter().for_each(|&item| self.resolve_expr(item)),
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
            Expr::Call { callee, args } => {
                self.bind_expr(id, callee, Expect::Func);
                args.iter().for_each(|&a| self.resolve_expr(a));
            }
            // The method itself depends on the receiver's type, so the type checker finds it.
            Expr::MethodCall { receiver, args, .. } => {
                self.resolve_expr(*receiver);
                args.iter().for_each(|&a| self.resolve_expr(a));
            }
            Expr::Field { object, .. } => self.resolve_expr(*object),
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
            Pattern::Wildcard | Pattern::Int(_) | Pattern::Bool(_) | Pattern::None => {}
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

    /// Whether `name` is a variant of an enum in the current module or one it uses.
    fn is_variant_name(&self, name: &str) -> bool {
        std::iter::once(self.module)
            .chain(self.imports[self.module].iter().copied())
            .any(|m| self.variant_names[m].contains(name))
    }

    fn bind_expr(&mut self, id: Id<Expr>, path: &Path, expect: Expect) {
        if let Some(def) = self.resolve_path(path, expect) {
            self.names.set_expr(id, def);
        }
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
        let found = if path.segments.len() == 1 {
            self.lookup(&path.segments[0].name, expect)
        } else {
            self.lookup_qualified(path, expect)
        };
        if found.is_none() {
            let error = self.not_found(path, expect);
            self.errors.push(error);
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
        // A value parameter is a value too: `N` in `struct Buffer<N: usize>`.
        if let Some(&(_, def)) = self.type_params.iter().find(|(n, _)| *n == name)
            && (expect == Expect::Type
                || (expect == Expect::Value && self.names.is_value_param(def)))
        {
            return Some(def);
        }

        // The standard library's root module, `som`, is seen from everywhere: the traits the
        // language uses are there.
        let prelude = self.modules.get("som").copied();
        std::iter::once(self.module)
            .chain(self.imports[self.module].iter().copied())
            .chain(prelude)
            .find_map(|m| self.items[m].get(name).copied())
            .filter(|&def| self.fits(def, expect))
    }

    /// `module::item`, `Struct::method`, or `module::Struct::method`.
    fn lookup_qualified(&self, path: &Path, expect: Expect) -> Option<DefId> {
        let segments = &path.segments;

        // The longest prefix that names a module, if any. Module names can contain `::`.
        let module_prefix = (1..segments.len()).rev().find_map(|len| {
            let name = segments[..len]
                .iter()
                .map(|s| &*s.name)
                .collect::<Vec<_>>()
                .join("::");
            self.modules.get(name.as_str()).map(|&m| (m, len))
        });

        let (owner, rest) = match module_prefix {
            Some((m, len)) => {
                let rest = &segments[len..];
                if rest.len() == 1 {
                    return self.items[m]
                        .get(&*rest[0].name)
                        .copied()
                        .filter(|&d| self.fits(d, expect));
                }
                (self.items[m].get(&*rest[0].name).copied()?, &rest[1..])
            }
            None => (
                self.lookup(&segments[0].name, Expect::Struct)?,
                &segments[1..],
            ),
        };

        match rest {
            [name] if matches!(self.names.def(owner).kind, DefKind::Struct(_)) => (self.names)
                .variant(owner, &name.name)
                .or_else(|| self.names.method(owner, &name.name))
                .filter(|&d| self.fits(d, expect)),
            _ => None,
        }
    }

    /// Whether `def` is something `expect` accepts.
    fn fits(&self, def: DefId, expect: Expect) -> bool {
        let kind = &self.names.def(def).kind;
        match expect {
            Expect::Value => !matches!(
                kind,
                DefKind::Struct(_)
                    | DefKind::TypeParam { .. }
                    | DefKind::StructParam { .. }
                    | DefKind::Trait(_)
                    | DefKind::AssocType { .. }
                    | DefKind::ImplParam { .. }
                    | DefKind::SelfType { .. }
                    | DefKind::ImplType { .. }
            ),
            // `Shape::Circle(1.0)` and `Shape::Rect { … }` make a variant.
            Expect::Func => matches!(kind, DefKind::Func(_) | DefKind::Variant { .. }),
            Expect::Struct => matches!(kind, DefKind::Struct(_) | DefKind::Variant { .. }),
            Expect::Type => matches!(
                kind,
                DefKind::Struct(_)
                    | DefKind::TypeParam { .. }
                    | DefKind::StructParam { .. }
                    | DefKind::AssocType { .. }
                    | DefKind::ImplParam { .. }
                    | DefKind::SelfType { .. }
                    | DefKind::ImplType { .. }
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
        let change_to = |text: String| {
            vec![Fix::replace(
                msg!["change to ", code(&text)],
                span.clone(),
                text,
            )]
        };
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
            return best_match(name, &candidates).map(|m| {
                let text = format!("{qualifier}::{m}");
                (msg!["did you mean ", code(&text), "?"], change_to(text))
            });
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
                ": write ",
                code(&qualified),
                " or add ",
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
