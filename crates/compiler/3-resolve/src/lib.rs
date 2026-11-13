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
pub use names::{Def, DefId, DefKind, Names};

use som_ast::{Ast, Expr, Func, Id, Ident, Item, Path, Stmt, TypeExpr};
use som_base::diagnostics::{closest_match, snake_case};
use som_base::message::{self, Message, Part, code};
use som_base::{Fix, msg};
use std::collections::HashMap;

pub fn resolve(ast: &Ast) -> (Names, Vec<ResolveError>) {
    let mut resolver = Resolver {
        ast,
        names: Names::new(ast),
        errors: Vec::new(),
        modules: HashMap::new(),
        items: vec![HashMap::new(); ast.modules.len()],
        imports: vec![Vec::new(); ast.modules.len()],
        module: 0,
        scopes: Vec::new(),
        type_params: Vec::new(),
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
}

impl Expect {
    /// `name` tagged as the kind of thing this expects, for messages.
    fn name(self, name: &str) -> Part {
        match self {
            Expect::Value => message::var(name),
            Expect::Func => message::func(name),
            Expect::Struct => message::struct_name(name),
            Expect::Type => message::ty(name),
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
    /// The module whose code is being resolved.
    module: usize,
    /// Local scopes of the function being resolved, innermost last.
    scopes: Vec<HashMap<&'a str, DefId>>,
    /// Type parameters of the function being resolved.
    type_params: Vec<(&'a str, DefId)>,
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
                let Some(owner) = self.resolve_path(&block.target, Expect::Struct) else {
                    continue;
                };
                for &method in &block.methods {
                    let name = &ast.funcs[method].name;
                    let def = self.names.func(method);
                    if let Some(previous) = self.names.add_method(owner, &name.name, def) {
                        self.errors.push(ResolveError::DuplicateDefinition {
                            name: format!("{}::{}", block.target, name),
                            span: name.span.clone(),
                            previous: self.names.def(previous).span.clone(),
                        });
                    }
                }
            }
        }
    }

    // --- Phase 2: bodies ---

    fn resolve_bodies(&mut self) {
        let ast = self.ast;
        for (m, module) in ast.modules.iter().enumerate() {
            self.module = m;
            for item in &module.items {
                match item {
                    Item::Func(id) => self.resolve_func(*id),
                    Item::Extern(block) => block.funcs.iter().for_each(|&id| self.resolve_func(id)),
                    Item::Impl(block) => block.methods.iter().for_each(|&id| self.resolve_func(id)),
                    Item::Struct(id) => {
                        for field in &ast.structs[*id].fields {
                            self.resolve_type(field.ty);
                        }
                    }
                    Item::Use(_) => {}
                }
            }
        }
    }

    fn resolve_func(&mut self, id: Id<Func>) {
        let func = &self.ast.funcs[id];

        self.type_params.clear();
        for (index, param) in func.type_params.iter().enumerate() {
            let def = self.names.define(
                &param.name,
                DefKind::TypeParam { func: id, index },
                param.span.clone(),
                self.module,
            );
            self.names.push_type_param(id, def);
            self.type_params.push((&param.name, def));
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
            Stmt::Break => {}
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
            Expr::Not(inner)
            | Expr::Deref(inner)
            | Expr::Borrow { expr: inner, .. }
            | Expr::ArrayRepeat { value: inner, .. } => self.resolve_expr(*inner),
            Expr::Array(items) => items.iter().for_each(|&item| self.resolve_expr(item)),
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
            Expr::Construct { ty, fields } => {
                self.bind_expr(id, ty, Expect::Struct);
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
        }
    }

    fn bind_expr(&mut self, id: Id<Expr>, path: &Path, expect: Expect) {
        if let Some(def) = self.resolve_path(path, expect) {
            self.names.set_expr(id, def);
        }
    }

    fn resolve_type(&mut self, id: Id<TypeExpr>) {
        match &self.ast.types[id] {
            TypeExpr::Named(path) => {
                if let Some(def) = self.resolve_path(path, Expect::Type) {
                    self.names.set_type(id, def);
                }
            }
            TypeExpr::Reference { to, .. }
            | TypeExpr::Optional(to)
            | TypeExpr::Array { element: to, .. }
            | TypeExpr::Slice { element: to, .. } => self.resolve_type(*to),
            TypeExpr::Fn { params, returns } => {
                params.iter().for_each(|&p| self.resolve_type(p));
                if let Some(returns) = returns {
                    self.resolve_type(*returns);
                }
            }
            TypeExpr::Unit | TypeExpr::Primitive(_) | TypeExpr::Pointer => {}
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
        if expect == Expect::Type
            && let Some(&(_, def)) = self.type_params.iter().find(|(n, _)| *n == name)
        {
            return Some(def);
        }

        std::iter::once(self.module)
            .chain(self.imports[self.module].iter().copied())
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
            [method] if matches!(self.names.def(owner).kind, DefKind::Struct(_)) => self
                .names
                .method(owner, &method.name)
                .filter(|&d| self.fits(d, expect)),
            _ => None,
        }
    }

    /// Whether `def` is something `expect` accepts.
    fn fits(&self, def: DefId, expect: Expect) -> bool {
        let kind = &self.names.def(def).kind;
        match expect {
            Expect::Value => !matches!(kind, DefKind::Struct(_) | DefKind::TypeParam { .. }),
            Expect::Func => matches!(kind, DefKind::Func(_)),
            Expect::Struct => matches!(kind, DefKind::Struct(_)),
            Expect::Type => matches!(kind, DefKind::Struct(_) | DefKind::TypeParam { .. }),
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
                self.names
                    .methods_of(owner)
                    .into_iter()
                    .map(|(n, _)| n.to_string())
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
