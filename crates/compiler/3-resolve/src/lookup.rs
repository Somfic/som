//! Finding what a name or path refers to from where it's written.

use som_ast::{Ident, Path};
use som_base::message::code;
use som_base::{Fix, msg};

use crate::names::{DefId, DefKind};
use crate::{Expect, ResolveError, Resolver, visibility};

impl<'a> Resolver<'a> {
    /// Resolves `path`, reporting an error and returning `None` if it doesn't name an `expect`.
    pub(crate) fn resolve_path(&mut self, path: &Path, expect: Expect) -> Option<DefId> {
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
        let Some(def) = found else {
            let error = self.not_found(path, expect);
            self.errors.push(error);
            return None;
        };
        // Found, but not to be seen from here: `pub` or `mod` would let it be.
        let span = path.span();
        let viewer = visibility::Viewer {
            module: self.cx.module,
            file: span.source.identifier(),
            owner: self.cx.owner,
        };
        if let Some(hidden) = visibility::hidden(self.ast, &self.names, def, viewer) {
            self.errors.push(ResolveError::NotVisible {
                name: path.to_string(),
                span,
                hidden,
            });
        }
        Some(def)
    }

    pub(crate) fn lookup(&self, name: &str, expect: Expect) -> Option<DefId> {
        if expect == Expect::Value
            && let Some(&def) = self
                .cx
                .scopes
                .iter()
                .rev()
                .find_map(|scope| scope.get(name))
        {
            return Some(def);
        }
        // `Self` and associated types inside a trait or impl block.
        if expect == Expect::Type
            && let Some(&(_, def)) = self.cx.block_types.iter().find(|(n, _)| *n == name)
        {
            return Some(def);
        }
        // `Self { … }` and `Self::new()` in an impl block: the struct or built-in type it's for.
        if expect == Expect::Struct
            && name == som_lang::SELF_TYPE
            && let Some(&(_, def)) = self.cx.block_types.iter().find(|(n, _)| *n == name)
            && let DefKind::SelfType { impl_ } = self.names.def(def).kind
        {
            return self.names.impls[impl_].owner;
        }
        // A value parameter is a value too: `N` in `struct Buffer<N: usize>`.
        if let Some(&(_, def)) = self.cx.type_params.iter().find(|(n, _)| *n == name)
            && (expect == Expect::Type
                || (expect == Expect::Value && self.names.is_value_param(def)))
        {
            return Some(def);
        }
        self.visible_modules()
            .find_map(|m| self.modules.items[m].get(name).copied())
            .and_then(|def| self.fitting(def, expect))
    }

    /// The modules whose items are named without a qualifier here, nearest first: this one,
    /// those `use`d in enclosing blocks, those it `use`s, and the standard library's root
    /// module, `som`, where the traits the language uses are.
    fn visible_modules(&self) -> impl Iterator<Item = usize> + '_ {
        let prelude = self.modules.by_name.get(som_lang::PRELUDE_MODULE).copied();
        std::iter::once(self.cx.module)
            .chain(self.cx.block_imports.iter().rev().map(|&(_, m)| m))
            .chain(self.modules.imports[self.cx.module].iter().copied())
            .chain(prelude)
    }

    /// Whether `name` is a variant of an enum in a module visible here: `Less` of `Ordering`.
    pub(crate) fn is_variant_name(&self, name: &str) -> bool {
        self.visible_modules()
            .any(|m| self.modules.variant_names[m].contains(name))
    }

    /// The longest prefix of `segments` naming a module, with its length. Module names can
    /// contain `::`.
    fn module_prefix(&self, segments: &[Ident]) -> Option<(usize, usize)> {
        (1..segments.len()).rev().find_map(|len| {
            let name: Vec<&str> = segments[..len].iter().map(|s| &*s.name).collect();
            (self.modules.by_name.get(name.join("::").as_str())).map(|&m| (m, len))
        })
    }

    /// The library `path` names something of, `c` in `c::puts`, if this module has no
    /// `extern` for it.
    fn undeclared_library(&self, path: &Path) -> Option<&'a str> {
        let (module, _) = self.module_prefix(&path.segments)?;
        let library = self.modules.libraries.get(&module)?;
        (!self.modules.declared[self.cx.module].contains(&module)).then_some(*library)
    }

    /// `module::item`, `Struct::method`, or `module::Struct::method`.
    fn lookup_qualified(&self, path: &Path, expect: Expect) -> Option<DefId> {
        let segments = &path.segments;
        // A module this one `use`s is named by its last part too: `io::println` after
        // `use som::io;`.
        let module_prefix = self.module_prefix(segments).or_else(|| {
            let first = &*segments[0].name;
            (self.cx.block_imports.iter().rev().map(|&(_, m)| m))
                .chain(self.modules.imports[self.cx.module].iter().copied())
                .find(|&m| self.ast.modules[m].name.rsplit("::").next() == Some(first))
                .map(|m| (m, 1))
        });

        let (owner, rest) = match module_prefix {
            Some((m, len)) => {
                let rest = &segments[len..];
                let item = self.modules.items[m].get(&*rest[0].name).copied();
                if rest.len() == 1 {
                    return item.and_then(|d| self.fitting(d, expect));
                }
                let owner = item?;
                (
                    self.fitting(owner, Expect::Struct).unwrap_or(owner),
                    &rest[1..],
                )
            }
            None => {
                let first = &segments[0].name;
                let owner = (self.lookup(first, Expect::Struct))
                    .or_else(|| self.names.primitives_named(first))
                    .or_else(|| self.lookup(first, Expect::Trait))?;
                (owner, &segments[1..])
            }
        };

        let [name] = rest else {
            return None;
        };
        let name = &*name.name;
        let member = match self.names.def(owner).kind {
            DefKind::Primitive(_) => {
                (self.names.method(owner, name)).or_else(|| self.names.assoc_const(owner, name))
            }
            DefKind::Struct(_) => (self.names.variant(owner, name))
                .or_else(|| self.names.method(owner, name))
                .or_else(|| self.names.assoc_const(owner, name)),
            // `Hash::hash(&value, &mut hasher)`: a trait's method, for whichever type `self`
            // turns out to be.
            DefKind::Trait(t) => (self.ast.traits[t].methods.iter())
                .find(|&&m| &*self.ast.funcs[m].name.name == name)
                .map(|&m| self.names.func(m)),
            _ => None,
        };
        member.filter(|&d| self.fits(d, expect))
    }

    /// Whether `def` is something `expect` accepts.
    pub(crate) fn fits(&self, def: DefId, expect: Expect) -> bool {
        let kind = &self.names.def(def).kind;
        match expect {
            Expect::Value => !kind.is_type() && !matches!(kind, DefKind::Trait(_)),
            // `Shape::Circle(1.0)` and `Shape::Rect { … }` make a variant.
            Expect::Func => matches!(kind, DefKind::Func(_) | DefKind::Variant { .. }),
            Expect::Struct => matches!(kind, DefKind::Struct(_) | DefKind::Variant { .. }),
            // A constant can be an array's length: `[i32; W]`.
            Expect::Type => kind.is_type() || matches!(kind, DefKind::Const(_)),
            Expect::Trait => matches!(kind, DefKind::Trait(_)),
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
}
