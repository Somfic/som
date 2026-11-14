use crate::{Expr, Id, Ident, Path, Span, TypeExpr};
use std::path::PathBuf;

/// A top-level declaration in a module.
pub enum Item {
    Func(Id<Func>),
    Struct(Id<Struct>),
    Use(Use),
    Extern(Extern),
    Impl(Impl),
    Trait(Id<Trait>),
}

/// `use math;` or `use std::io;`
pub struct Use {
    pub path: Path,
}

/// `extern "lib" { fn puts(s: &str); }`: functions without a body, linked from `library`
/// (or from libc when no library is named).
pub struct Extern {
    pub library: Option<String>,
    /// Where the library's name is written, for errors about the library.
    pub library_span: Option<Span>,
    /// `extern "raylib" from "raylib.h"`: a C header to import declarations from, and
    /// where it's written.
    pub header: Option<(String, Span)>,
    /// `use extern "raylib" …`: also `use` the library's module.
    pub use_module: bool,
    /// The module the functions belong to, when it's not the one the block is written in:
    /// a library's functions go in a module named after it (`raylib::InitWindow`). Set
    /// while loading.
    pub module: Option<usize>,
    pub funcs: Vec<Id<Func>>,
}

/// `impl Point { fn sum(self: Point) -> i32 { ... } }`, or with a trait,
/// `impl<T> Iterator for ListIter<T> { type Item = &T; fn next(…) … }`.
pub struct Impl {
    /// The struct the methods are for: `Point`, or one from another module, `box2d::B2Vec2`.
    pub target: Path,
    /// The target's type arguments as written: `T` in `ListIter<T>`, `i32` in `List<i32>`.
    pub target_args: Vec<Id<TypeExpr>>,
    /// A trait's implementation for a type that isn't a struct: `bool` in `impl Add for bool`,
    /// `[T; N]`, `&T`. `target` is then just its text, for showing.
    pub target_ty: Option<Id<TypeExpr>>,
    /// `impl Iterator for …`: the trait implemented, and its type arguments.
    pub trait_: Option<(Path, Vec<Id<TypeExpr>>)>,
    /// `type Item = &T;`: what the trait's associated types are, for the target.
    pub types: Vec<(Ident, Id<TypeExpr>)>,
    /// `impl<T> Pair<T>`: generic parameters every method gets, ahead of its own.
    pub generics: Vec<Generic>,
    pub methods: Vec<Id<Func>>,
    /// Its closing brace, where missing methods go.
    pub close: som_base::Span,
}

/// `trait Iterator { type Item; fn next(self: &mut Self) -> Item?; }`: what a type can do,
/// which `impl Iterator for …` gives it, and a type parameter can require: `I: Iterator`.
pub struct Trait {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub name: Ident,
    pub generics: Vec<Generic>,
    /// `type Item;`: types each implementation says.
    pub types: Vec<AssocType>,
    /// Each method's signature, generic over `Self` (the implementing type) first, then the
    /// trait's own generics, then its own. One with a body is what implementations get if
    /// they don't write it.
    pub methods: Vec<Id<Func>>,
}

/// `type Item;` in a trait.
pub struct AssocType {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub name: Ident,
}

/// A function, method, or extern declaration (which has no `body`).
pub struct Func {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub params: Vec<Param>,
    pub returns: Option<Id<TypeExpr>>,
    pub body: Option<Id<Expr>>,
    /// An extern function taking extra arguments after its parameters, like C's `printf`.
    pub variadic: bool,
    /// For an extern function, the symbol it links to when that isn't its name: som's
    /// `init_window` is raylib's `InitWindow`. Worked out from the library while checking.
    pub link_name: Option<Box<str>>,
    /// For a function in an `extern "lib"` block, the library file it's linked from.
    /// Worked out while checking.
    pub library: Option<PathBuf>,
    /// A method of a trait: a signature implementations follow, not a function of its own.
    pub in_trait: bool,
}

/// A generic parameter: a type, `T` in `struct Pair<T>`, or a value of a type, `N: usize` in
/// `struct Buffer<N: usize>`. A type can have to implement traits: `T: Hash + Eq`.
#[derive(Clone)]
pub struct Generic {
    pub name: Ident,
    /// The value's type, for a value parameter: a primitive type after the `:`.
    pub ty: Option<Id<TypeExpr>>,
    /// The traits a type parameter has to implement: anything else after the `:`.
    pub bounds: Vec<Path>,
}

pub struct Param {
    pub name: Ident,
    pub ty: Option<Id<TypeExpr>>,
    /// `out remainder: i32`: the function fills it in, and callers pass `out name`.
    /// Passed as a pointer, like `&mut i32`.
    pub out: bool,
    /// `mut count: i32`: the function can assign it. An `out` parameter always can.
    pub mutable: bool,
}

pub struct Struct {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub name: Ident,
    /// `struct Pair<T>`, `struct Buffer<N: usize>`: what it's written in terms of, filled in
    /// where it's used.
    pub generics: Vec<Generic>,
    pub fields: Vec<Field>,
    /// `enum Shape { … }`: the variants, one of which each value is. An enum has no `fields`
    /// of its own; each variant has its own.
    pub variants: Option<Vec<Variant>>,
    /// Values are copied rather than moved. Set for structs imported from a C header,
    /// which are plain data, and for enums whose variants hold nothing.
    pub copy: bool,
    /// `extern struct Sqlite3;`: a struct only C sees inside. Its values are handles,
    /// pointers to one, which som can pass around but not look into or create.
    pub handle: bool,
}

impl Struct {
    pub fn is_enum(&self) -> bool {
        self.variants.is_some()
    }

    /// An enum's variants; none for a struct.
    pub fn variants(&self) -> &[Variant] {
        self.variants.as_deref().unwrap_or(&[])
    }

    /// Every variant's fields, one variant after another: how an enum's payload is laid out.
    pub fn payload(&self) -> impl Iterator<Item = &Field> {
        self.variants().iter().flat_map(|v| &v.fields)
    }

    /// Where variant `variant`'s fields start in `payload`.
    pub fn payload_start(&self, variant: usize) -> usize {
        self.variants()[..variant]
            .iter()
            .map(|v| v.fields.len())
            .sum()
    }
}

/// One of an enum's variants: `Empty`, `Circle(f32)` or `Rect { w: f32, h: f32 }`.
pub struct Variant {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub name: Ident,
    pub kind: VariantKind,
    /// What it holds: named fields for `Rect { w: f32 }`, or numbered `0`, `1`, … for
    /// `Circle(f32)`.
    pub fields: Vec<Field>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantKind {
    /// `Empty`: holds nothing.
    Unit,
    /// `Circle(f32)`: fields by position.
    Tuple,
    /// `Rect { w: f32, h: f32 }`: fields by name.
    Named,
}

pub struct Field {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub name: Ident,
    pub ty: Id<TypeExpr>,
}
