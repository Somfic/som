use crate::{Expr, Id, Ident, Path, Span, TypeExpr};
use std::path::PathBuf;

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

/// Its value is made of literals, operators, casts, other constants and arrays or structs of
/// those; each use stands for it.
pub struct Const {
    pub doc: Option<String>,
    pub visibility: Visibility,
    pub name: Ident,
    pub ty: Option<Id<TypeExpr>>,
    pub value: Id<Expr>,
    /// `@const f(x)` written as an expression ([`Expr::InPlace`]), which gives its type.
    pub in_place: bool,
}

impl Const {
    /// The name of one written in place, or of a check on its own: `@assert(…);`.
    pub const UNNAMED: &'static str = som_lang::Builtin::Const.name();

    pub fn is_named(&self) -> bool {
        &*self.name.name != Self::UNNAMED
    }
}

/// `mod`: its module, the folder it's in. Private: its own file, and for a field, method or
/// property also its type's `impl` blocks, wherever they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Visibility {
    #[default]
    Private,
    Module,
    Public,
}

pub struct Alias {
    pub doc: Option<String>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub ty: Id<TypeExpr>,
}

pub struct Use {
    pub path: Path,
    /// `pub use som::collections;`: whoever sees this module sees what it uses too, as if
    /// written here.
    pub public: bool,
}

/// Linked from `library`, or from libc when no library is named.
pub struct Extern {
    /// From `extern` to the closing `}` or `;`, not counting a `use` before it.
    pub span: Span,
    pub library: Option<String>,
    pub library_span: Option<Span>,
    /// `extern "raylib" from "raylib.h"`, and where it's written.
    pub header: Option<(String, Span)>,
    pub use_module: bool,
    /// Set while loading, when it's not the one the block is written in: a library's
    /// functions go in a module named after it (`raylib::InitWindow`).
    pub module: Option<usize>,
    pub funcs: Vec<Id<Func>>,
}

pub struct Impl {
    pub target: Path,
    pub target_args: Vec<Id<TypeExpr>>,
    /// A type that isn't a struct: `bool` in `impl Add for bool`. `target` is then just its
    /// text, for showing.
    pub target_ty: Option<Id<TypeExpr>>,
    pub trait_: Option<(Path, Vec<Id<TypeExpr>>)>,
    pub types: Vec<(Ident, Id<TypeExpr>)>,
    pub generics: Vec<Generic>,
    pub methods: Vec<Id<Func>>,
    /// Named through the type, `f32::PI`, or `Self::PI` inside.
    pub consts: Vec<Id<Const>>,
    /// Where missing methods go.
    pub close: som_base::Span,
}

pub struct Trait {
    pub doc: Option<String>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub types: Vec<AssocType>,
    /// `type Predicate = fn(&Self::Item) -> bool;`: names the trait gives types, in terms of
    /// `Self` and its parameters. Implementations don't write them.
    pub aliases: Vec<TraitAlias>,
    /// `trait IndexMut: Index`: whatever implements it implements these too.
    pub supers: Vec<Path>,
    /// Generic over `Self` first, then the trait's generics, then its own. One with a body
    /// is what implementations get if they don't write it.
    pub methods: Vec<Id<Func>>,
}

pub struct AssocType {
    pub doc: Option<String>,
    pub name: Ident,
}

pub struct TraitAlias {
    pub doc: Option<String>,
    pub name: Ident,
    pub ty: Id<TypeExpr>,
}

pub struct Func {
    pub doc: Option<String>,
    /// A trait's methods, and its implementations', are as visible as the trait.
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub params: Vec<Param>,
    pub returns: Option<Id<TypeExpr>>,
    pub body: Option<Id<Expr>>,
    pub variadic: bool,
    /// The extern symbol, when that isn't its name: som's `init_window` is raylib's
    /// `InitWindow`. Worked out while checking.
    pub link_name: Option<Box<str>>,
    /// Worked out while checking.
    pub library: Option<PathBuf>,
    /// A signature implementations follow, not a function of its own.
    pub in_trait: bool,
    /// `prop len: usize`, read as `list.len`, taking an unwritten `&self`. Properties are
    /// named apart from methods, so a type can have a `len` of each.
    pub prop: bool,
    /// `@where i < self.len`: known to hold wherever it's called.
    pub requires: Vec<Where>,
    pub listens: Vec<Ident>,
    /// `@test`: run by `som test`, and otherwise reached from nothing.
    pub test: bool,
    /// `@if N >= 3 { pub prop z … }` around it, among an impl block's members: what has to
    /// hold, of the block's parameters, for a type to have it.
    pub when: Vec<Id<Expr>>,
}

/// `@where i < self.len, "a position in it"`: the reason is for error messages.
#[derive(Clone)]
pub struct Where {
    pub condition: Id<Expr>,
    pub reason: Option<Box<str>>,
}

/// `T`, `T: Hash + Eq`, or a value parameter, `N: usize`.
#[derive(Clone)]
pub struct Generic {
    pub name: Ident,
    pub ty: Option<Id<TypeExpr>>,
    pub bounds: Vec<Bound>,
    /// `F: fn(&T) -> bool`: what calling an `F` takes and gives. A function, or a function's
    /// address, that does.
    pub signature: Option<Id<TypeExpr>>,
    /// `Self` in `trait Add<Other = Self>`. Only a struct's or a trait's parameters have one.
    pub default: Option<Id<TypeExpr>>,
}

/// A trait a generic parameter has to implement: `Hash`, or `Call<&I::Item, bool>`.
#[derive(Clone)]
pub struct Bound {
    pub trait_: Path,
    /// What's left out is the trait's default.
    pub args: Vec<Id<TypeExpr>>,
    /// The trait and its arguments, `>` included.
    pub span: Span,
}

impl Bound {
    /// Just a trait, nothing given: `Self: Trait` in a trait's own methods.
    pub fn of(trait_: Path) -> Bound {
        let span = trait_.span();
        Bound {
            trait_,
            args: Vec::new(),
            span,
        }
    }
}

pub struct Param {
    pub name: Ident,
    pub ty: Option<Id<TypeExpr>>,
    /// `out remainder: i32`: passed as a pointer, like `&mut i32`, for the function to fill
    /// in.
    pub out: bool,
    pub mutable: bool,
}

pub struct Struct {
    pub doc: Option<String>,
    pub visibility: Visibility,
    pub name: Ident,
    pub generics: Vec<Generic>,
    pub fields: Vec<Field>,
    /// An enum's: it has no `fields` of its own; each variant has its own.
    pub variants: Option<Vec<Variant>>,
    /// Set for structs imported from a C header, which are plain data, and for enums whose
    /// variants hold nothing.
    pub copy: bool,
    /// `extern struct Sqlite3;`: only C sees inside. Its values are pointers to one, which
    /// som can pass around but not look into or create.
    pub handle: bool,
}

impl Struct {
    pub fn is_enum(&self) -> bool {
        self.variants.is_some()
    }

    pub fn variants(&self) -> &[Variant] {
        self.variants.as_deref().unwrap_or(&[])
    }

    /// Every variant's fields, one variant after another: how the payload is laid out.
    pub fn payload(&self) -> impl Iterator<Item = &Field> {
        self.variants().iter().flat_map(|v| &v.fields)
    }

    /// What `as` converts it to: the number written after it, `W = 87`, or as in C, one more
    /// than the variant before it's (the first's is 0).
    pub fn variant_value(&self, variant: usize) -> i128 {
        let variants = self.variants();
        let mut value = -1;
        for v in &variants[..=variant] {
            value = v.value.unwrap_or(value + 1);
        }
        value
    }

    pub fn has_values(&self) -> bool {
        self.variants().iter().any(|v| v.value.is_some())
    }

    pub fn payload_start(&self, variant: usize) -> usize {
        self.variants()[..variant]
            .iter()
            .map(|v| v.fields.len())
            .sum()
    }
}

pub struct Variant {
    pub doc: Option<String>,
    pub name: Ident,
    pub kind: VariantKind,
    /// `W = 87`, of a variant holding nothing. Only what `as` gives changes; which variant
    /// a value is stays as it is.
    pub value: Option<i128>,
    /// Numbered `0`, `1`, … for `Circle(f32)`.
    pub fields: Vec<Field>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantKind {
    Unit,
    Tuple,
    Named,
}

pub struct Field {
    pub doc: Option<String>,
    /// A variant's fields are as visible as the enum.
    pub visibility: Visibility,
    pub name: Ident,
    pub ty: Id<TypeExpr>,
}
