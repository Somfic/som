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
    Alias(Alias),
    Const(Id<Const>),
}

/// `@const W: i32 = 60;`: a value known while compiling, named anywhere in its module (and
/// further, with `pub` or `mod`). Its value is made of literals, operators, casts, other
/// constants and arrays or structs of those; each use stands for it.
pub struct Const {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub visibility: Visibility,
    pub name: Ident,
    pub ty: Option<Id<TypeExpr>>,
    pub value: Id<Expr>,
}

/// `type Vector3 = Vector<3, f32>;`: another name for a type, which stands for it wherever
/// it's written. It can have generic parameters, filled in where it's used:
/// `type Vector3<T> = Vector<3, T>;`.
/// Who can see an item. `pub`: everyone. `mod`: its module, the folder it's in. Without
/// either: its own file, and for a field, method or property also its type's `impl` blocks,
/// wherever they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Visibility {
    #[default]
    Private,
    Module,
    Public,
}

pub struct Alias {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub ty: Id<TypeExpr>,
}

/// `use math;` or `use std::io;`
pub struct Use {
    pub path: Path,
}

/// `extern "lib" { fn puts(s: &str); }`: functions without a body, linked from `library`
/// (or from libc when no library is named).
pub struct Extern {
    /// From `extern` to the closing `}` or `;`, not counting a `use` before it.
    pub span: Span,
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
    /// `@const PI = 3.14;`: constants named through the type, `f32::PI`, or `Self::PI` inside.
    pub consts: Vec<Id<Const>>,
    /// Its closing brace, where missing methods go.
    pub close: som_base::Span,
}

/// `trait Iterator { type Item; fn next(self: &mut Self) -> Item?; }`: what a type can do,
/// which `impl Iterator for …` gives it, and a type parameter can require: `I: Iterator`.
pub struct Trait {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    /// `type Item;`: types each implementation says.
    pub types: Vec<AssocType>,
    /// `trait IndexMut: Index`: traits whatever implements it implements too, whose
    /// methods and properties its own can use.
    pub supers: Vec<Path>,
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
    /// For a method in a trait, or a trait's implementation: as visible as the trait.
    pub visibility: Visibility,
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
    /// `prop len: usize`: a property, read without parentheses as `list.len`. It takes
    /// `&self`, which isn't written. Properties are named apart from methods, so a type can
    /// have a `len` of each.
    pub prop: bool,
    /// `@where i < self.len` above it: what has to be known to hold wherever it's called,
    /// which its body can rely on.
    pub requires: Vec<Where>,
}

/// `@where i < self.len, "a position in it"`: a condition on a function's parameters, with
/// what it means, for error messages.
#[derive(Clone)]
pub struct Where {
    pub condition: Id<Expr>,
    pub reason: Option<Box<str>>,
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
    /// The type it is when it's left out, after the `=`: `Other` in
    /// `trait Add<Other = Self>`. Only a struct's or a trait's parameters have one.
    pub default: Option<Id<TypeExpr>>,
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
    pub visibility: Visibility,
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

    /// The number variant `variant` converts to with `as`: the one written after it,
    /// `W = 87`, or as in C, one more than the variant before it's (the first's is 0).
    pub fn variant_value(&self, variant: usize) -> i128 {
        let variants = self.variants();
        let mut value = -1;
        for v in &variants[..=variant] {
            value = v.value.unwrap_or(value + 1);
        }
        value
    }

    /// Whether some variant has a number written after it, `W = 87`, so they don't all
    /// convert to their position.
    pub fn has_values(&self) -> bool {
        self.variants().iter().any(|v| v.value.is_some())
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
    /// `W = 87`: the number it converts to with `as`, of a variant holding nothing. Only
    /// what converts changes; which variant a value is stays as it is.
    pub value: Option<i128>,
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
    /// An enum variant's fields are as visible as the enum.
    pub visibility: Visibility,
    pub name: Ident,
    pub ty: Id<TypeExpr>,
}
