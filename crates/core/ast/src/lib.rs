//! The syntax tree, exactly as written. Later passes keep what they work out in their own
//! side tables, keyed by `Id`.

mod dump;
mod expr;
mod flow;
mod item;
mod nodes;
mod ops;
mod ty;
mod walk;
mod written;

pub use expr::*;
pub use flow::{Flow, Settled};
pub use item::*;
pub use nodes::Nodes;
pub use ty::*;
pub use walk::Child;

pub use som_base::{Id, Source, Span};

use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Default)]
pub struct Ast {
    pub modules: Vec<Module>,
    pub funcs: Nodes<Func>,
    pub structs: Nodes<Struct>,
    pub exprs: Nodes<Expr>,
    pub stmts: Nodes<Stmt>,
    pub types: Nodes<TypeExpr>,
    pub patterns: Nodes<Pattern>,
    pub traits: Nodes<Trait>,
    pub consts: Nodes<Const>,
    /// Statements that were a block's last line without a `;`, where the block's value isn't
    /// used: `end_drawing()` in `while … { begin_drawing(); end_drawing() }`.
    pub unterminated: std::collections::HashSet<Id<Stmt>>,
    /// `@for` and `@if`: settled while compiling.
    pub settled: std::collections::HashSet<Id<Stmt>>,
    /// `@if`s on the target's system, decided while loading: whether the first branch is
    /// the one compiled. The other is never resolved, so it can name what only exists on
    /// other systems.
    pub decided: std::collections::HashMap<Id<Stmt>, bool>,
    /// `@if GPU`s: whether the first branch is the GPU's. Both branches are resolved and
    /// checked; a function compiled for the GPU (for a shader) takes the GPU's, and the
    /// program on the CPU the other.
    pub gpu_ifs: std::collections::HashMap<Id<Stmt>, bool>,
    /// Written out once every module is loaded (`som_parser::expand_reflected`).
    pub reflected: Vec<Reflected>,
    /// `@for f in T { … }` over a generic `T`, among statements: written out for every
    /// struct, each instance of the function picking its copy.
    pub per_type: std::collections::HashMap<Id<Stmt>, PerType>,
    /// `match v { @for (i, k) in T { k => i, } }` over a generic `T`: the whole `match`
    /// written out for every enum, each instance picking its copy.
    pub per_type_matches: std::collections::HashMap<Id<Expr>, PerType>,
}

pub struct PerType {
    pub of: Path,
    pub arms: Vec<(Id<Struct>, Id<Expr>)>,
}

/// `@for (i, v) in Kind { v => i, }` over an enum's variants, or `@for f in Point { … }` over
/// a struct's fields.
pub struct Reflected {
    pub module: usize,
    /// `v`, or `(i, v)`.
    pub names: Vec<Ident>,
    pub of: Path,
    /// `@for f in @leaves(T)`: into the fields of the program's own structs too.
    pub leaves: bool,
    /// Between the braces.
    pub body: Span,
    pub into: ReflectedInto,
}

pub enum ReflectedInto {
    Arms {
        expr: Id<Expr>,
        at: usize,
    },
    /// In place of `stmt`, which stands for it until it's written out.
    Stmts {
        stmt: Id<Stmt>,
    },
}

/// The `.som` files in one directory, named by the `use` path that loaded it. A program
/// parsed from a single source has one module with an empty name.
pub struct Module {
    pub name: Box<str>,
    pub path: PathBuf,
    pub items: Vec<Item>,
    pub files: Vec<Arc<Source>>,
    /// Loaded like its own `use`s.
    pub block_uses: Vec<Path>,
}

impl Ast {
    pub fn add_module(&mut self, name: impl Into<Box<str>>, path: PathBuf) -> usize {
        self.modules.push(Module {
            name: name.into(),
            path,
            items: Vec::new(),
            files: Vec::new(),
            block_uses: Vec::new(),
        });
        self.modules.len() - 1
    }

    /// And whether any block links libc.
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    pub segments: Vec<Ident>,
}

impl Path {
    pub fn name(&self) -> &Ident {
        self.segments.last().expect("paths are never empty")
    }

    pub fn qualifier(&self) -> &[Ident] {
        &self.segments[..self.segments.len() - 1]
    }

    pub fn span(&self) -> Span {
        let first = &self.segments[0].span;
        first.merge(&self.name().span)
    }

    /// `usize::MAX`, `i32::MIN`, and its value: for `usize` and `isize`, as 64 bits (C writes
    /// theirs as the target's), and `u128::MAX` as the `i128` with the same bits, `-1`.
    pub fn int_limit(&self) -> Option<(Primitive, i128)> {
        let [ty, limit] = self.segments.as_slice() else {
            return None;
        };
        let primitive = Primitive::from_name(&ty.name)?;
        let int = primitive.int()?;
        match &*limit.name {
            "MIN" => Some((primitive, int.min())),
            "MAX" => Some((primitive, int.max() as i128)),
            _ => None,
        }
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
