//! The syntax tree, exactly as written. No names are resolved and no types are
//! known here; later passes attach that information in their own side tables,
//! keyed by the `Id`s handed out below.

mod dump;
mod expr;
mod item;
mod nodes;
mod ty;

pub use expr::*;
pub use item::*;
pub use nodes::Nodes;
pub use ty::*;

pub use som_base::{Id, Source, Span};

use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Arc;

/// Every node in a program, across all of its modules.
#[derive(Default)]
pub struct Ast {
    pub modules: Vec<Module>,
    pub funcs: Nodes<Func>,
    pub structs: Nodes<Struct>,
    pub exprs: Nodes<Expr>,
    pub stmts: Nodes<Stmt>,
    pub types: Nodes<TypeExpr>,
}

/// One module: the `.som` files in one directory, named by the `use` path that loaded it.
/// A program parsed from a single source has one module with an empty name.
pub struct Module {
    pub name: Box<str>,
    pub path: PathBuf,
    pub items: Vec<Item>,
    /// The source files the module was parsed from, in order.
    pub files: Vec<Arc<Source>>,
}

impl Ast {
    /// Adds an empty module and returns its index.
    pub fn add_module(&mut self, name: impl Into<Box<str>>, path: PathBuf) -> usize {
        self.modules.push(Module {
            name: name.into(),
            path,
            items: Vec::new(),
            files: Vec::new(),
        });
        self.modules.len() - 1
    }

    /// Libraries named by `extern "lib" { ... }` blocks, and whether any block links libc.
    pub fn extern_libraries(&self) -> (Vec<String>, bool) {
        let mut libraries = Vec::new();
        let mut needs_libc = false;
        for item in self.modules.iter().flat_map(|m| &m.items) {
            if let Item::Extern(block) = item {
                match &block.library {
                    Some(lib) => libraries.push(lib.clone()),
                    None => needs_libc = true,
                }
            }
        }
        (libraries, needs_libc)
    }
}

/// A name as written in the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident {
    pub name: Box<str>,
    pub span: Span,
}

impl Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// A possibly qualified name: `foo`, `math::add`, `std::String::new`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    pub segments: Vec<Ident>,
}

impl Path {
    /// The last segment.
    pub fn name(&self) -> &Ident {
        self.segments.last().expect("paths are never empty")
    }

    /// Everything before the last segment.
    pub fn qualifier(&self) -> &[Ident] {
        &self.segments[..self.segments.len() - 1]
    }

    pub fn span(&self) -> Span {
        let first = &self.segments[0].span;
        first.merge(&self.name().span)
    }
}

impl Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, segment) in self.segments.iter().enumerate() {
            if i > 0 {
                f.write_str("::")?;
            }
            f.write_str(&segment.name)?;
        }
        Ok(())
    }
}
