//! Name resolution: binds every name in the syntax tree to the definition it refers to. This is
//! the only place in the compiler that looks names up by string; every later pass works with
//! `DefId`s.
//!
//! Lookup rules:
//! - An unqualified name is looked up in the enclosing local scopes, then in the current module,
//!   then in the modules it `use`s, in order.
//! - A qualified name starts with a module (`math::add`), a struct (`Point::new`), or both
//!   (`geo::Point::new`).
//! - Items can be used before they are defined; locals cannot.

mod bodies;
mod collect;
mod consts;
mod error;
mod lookup;
mod names;
mod suggest;
pub mod visibility;

pub use consts::const_runs_code;
pub use error::ResolveError;
pub use names::{Def, DefId, DefKind, ImplDef, Names};

use som_ast::{Ast, Ident, Item};
use som_base::message::{self, Part};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

pub fn resolve(ast: &Ast) -> (Names, Vec<ResolveError>) {
    let mut resolver = Resolver {
        ast,
        names: Names::new(ast),
        errors: Vec::new(),
        modules: Modules::new(ast),
        cx: Context::default(),
        through_alias: Cell::new(None),
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

/// What's known about each module, by index into `ast.modules`.
struct Modules<'a> {
    by_name: HashMap<&'a str, usize>,
    /// Its functions, structs, traits, aliases and constants, by name.
    items: Vec<HashMap<&'a str, DefId>>,
    /// The modules it `use`s, in order.
    imports: Vec<Vec<usize>>,
    /// The modules it `pub use`s: seen, wherever it is, as if their items were its own.
    reexports: Vec<Vec<usize>>,
    /// Its enums' variant names. A bare name in a pattern that's one of those, here or in a
    /// module this one uses, is that variant rather than a new binding.
    variant_names: Vec<HashSet<&'a str>>,
    /// Libraries' modules → the library: `c` for `c::puts`.
    libraries: HashMap<usize, &'a str>,
    /// The libraries' modules it has an `extern` for, which it can name.
    declared: Vec<HashSet<usize>>,
}

impl<'a> Modules<'a> {
    /// `modules`, each followed by what it passes on through `pub use`, and what those pass
    /// on, each once: a module passing on one that passes it back stops there.
    fn with_reexports(&self, modules: impl Iterator<Item = usize>) -> Vec<usize> {
        let mut seen = Vec::new();
        let mut todo: Vec<usize> = modules.collect();
        todo.reverse();
        while let Some(m) = todo.pop() {
            if seen.contains(&m) {
                continue;
            }
            seen.push(m);
            todo.extend(self.reexports[m].iter().rev());
        }
        seen
    }

    fn new(ast: &'a Ast) -> Self {
        let count = ast.modules.len();
        let mut modules = Modules {
            by_name: HashMap::new(),
            items: vec![HashMap::new(); count],
            imports: vec![Vec::new(); count],
            reexports: vec![Vec::new(); count],
            variant_names: vec![HashSet::new(); count],
            libraries: HashMap::new(),
            declared: vec![HashSet::new(); count],
        };
        // A library's module, `c`, is named only where its `extern "c"` is written.
        for (m, module) in ast.modules.iter().enumerate() {
            for item in &module.items {
                if let Item::Extern(block) = item
                    && let (Some(library), Some(lib)) = (&block.library, block.module)
                {
                    modules.libraries.insert(lib, library.as_str());
                    modules.declared[m].insert(lib);
                }
            }
        }
        modules
    }
}

/// Where the code being resolved is: what's in scope there.
#[derive(Default)]
struct Context<'a> {
    module: usize,
    /// Local scopes of the function being resolved, innermost last.
    scopes: Vec<HashMap<&'a str, DefId>>,
    /// The modules `use`d inside the blocks being resolved, with how many scopes deep each
    /// `use` is: gone when its block ends.
    block_imports: Vec<(usize, usize)>,
    type_params: Vec<(&'a str, DefId)>,
    /// Types named inside a trait or impl block besides its parameters: `Self`, and its
    /// associated types.
    block_types: Vec<(&'a str, DefId)>,
    /// The type whose impl block is being resolved: what its members' privacy is to.
    owner: Option<DefId>,
    /// Resolving a `@const` inside a function: how many scopes were open where it starts.
    /// Its value can't name the locals in those, only its own.
    const_depth: Option<usize>,
    /// Resolving an `@if`'s condition, where a type can be compared with another: `T == f32`.
    settled_condition: bool,
}

struct Resolver<'a> {
    ast: &'a Ast,
    names: Names,
    errors: Vec<ResolveError>,
    modules: Modules<'a>,
    cx: Context<'a>,
    /// The alias the last path looked up named its struct through, if it did.
    through_alias: Cell<Option<DefId>>,
}

impl<'a> Resolver<'a> {
    fn define_at(&mut self, name: &Ident, kind: DefKind, module: usize) -> DefId {
        self.names
            .define(&name.name, kind, name.span.clone(), module)
    }

    fn define(&mut self, name: &Ident, kind: DefKind) -> DefId {
        self.define_at(name, kind, self.cx.module)
    }

    fn duplicate(&mut self, name: &Ident, previous: DefId) {
        self.errors.push(ResolveError::DuplicateDefinition {
            name: name.name.to_string(),
            span: name.span.clone(),
            previous: self.names.def(previous).span.clone(),
        });
    }

    fn bind(&mut self, name: &'a Ident, def: DefId) {
        (self.cx.scopes.last_mut())
            .expect("inside a scope")
            .insert(&name.name, def);
    }

    fn in_scope(&mut self, f: impl FnOnce(&mut Self)) {
        self.cx.scopes.push(HashMap::new());
        f(self);
        self.pop_scope();
    }

    /// Ends the innermost scope, and what its `use`s brought in.
    fn pop_scope(&mut self) {
        self.cx.scopes.pop();
        let depth = self.cx.scopes.len();
        self.cx.block_imports.retain(|&(d, _)| d <= depth);
    }
}
