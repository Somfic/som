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
    pub patterns: Nodes<Pattern>,
    pub traits: Nodes<Trait>,
    pub consts: Nodes<Const>,
    /// Statements that were a block's last line without a `;`, where the block's value isn't
    /// used: `end_drawing()` in `while … { begin_drawing(); end_drawing() }`.
    pub unterminated: std::collections::HashSet<Id<Stmt>>,
    /// `@for` and `@if`: a `Stmt::For` or `Stmt::If` settled while compiling. An `@for`'s
    /// body is copied once for each value, and only the branch an `@if` picks exists.
    pub settled: std::collections::HashSet<Id<Stmt>>,
    /// `@for (i, v) in Kind { … }`, which can only be written out once every module is
    /// loaded and `Kind` is known (`som_parser::expand_reflected`).
    pub reflected: Vec<Reflected>,
}

/// An `@for` over a type: `@for (i, v) in Kind { v => i, }`, each variant of enum `Kind` in
/// turn, `i` its index.
pub struct Reflected {
    /// The module it's written in.
    pub module: usize,
    /// What each copy calls the element: `v`, or its index and it, `(i, v)`.
    pub names: Vec<Ident>,
    /// The type it goes through: `Kind`.
    pub of: Path,
    /// Its body, between the braces: copied once for each element.
    pub body: Span,
    /// Where the copies go.
    pub into: ReflectedInto,
}

/// Where an `@for` over a type's copies go.
pub enum ReflectedInto {
    /// Among a `match`'s arms, before the one at `at`.
    Arms { expr: Id<Expr>, at: usize },
}

/// One module: the `.som` files in one directory, named by the `use` path that loaded it.
/// A program parsed from a single source has one module with an empty name.
pub struct Module {
    pub name: Box<str>,
    pub path: PathBuf,
    pub items: Vec<Item>,
    /// The source files the module was parsed from, in order.
    pub files: Vec<Arc<Source>>,
    /// The paths `use`d inside its blocks, which are loaded like its own `use`s.
    pub block_uses: Vec<Path>,
}

impl Ast {
    /// Adds an empty module and returns its index.
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

    /// `usize::MAX`, `i32::MIN`: the integer type's limit it names, and its value: for
    /// `usize` and `isize`, as 64 bits (C writes theirs as the target's), and `u128::MAX` as
    /// the `i128` with the same bits, `-1`.
    pub fn int_limit(&self) -> Option<(Primitive, i128)> {
        let [ty, limit] = self.segments.as_slice() else {
            return None;
        };
        let (primitive, min, max): (Primitive, i128, i128) = match &*ty.name {
            "i8" => (Primitive::I8, i8::MIN as i128, i8::MAX as i128),
            "i16" => (Primitive::I16, i16::MIN as i128, i16::MAX as i128),
            "i32" => (Primitive::I32, i32::MIN as i128, i32::MAX as i128),
            "i64" => (Primitive::I64, i64::MIN as i128, i64::MAX as i128),
            "i128" => (Primitive::I128, i128::MIN, i128::MAX),
            "isize" => (Primitive::ISize, i64::MIN as i128, i64::MAX as i128),
            "u8" => (Primitive::U8, 0, u8::MAX as i128),
            "u16" => (Primitive::U16, 0, u16::MAX as i128),
            "u32" => (Primitive::U32, 0, u32::MAX as i128),
            "u64" => (Primitive::U64, 0, u64::MAX as i128),
            "u128" => (Primitive::U128, 0, u128::MAX as i128),
            "usize" => (Primitive::USize, 0, u64::MAX as i128),
            _ => return None,
        };
        match &*limit.name {
            "MIN" => Some((primitive, min)),
            "MAX" => Some((primitive, max)),
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
