use crate::{Expr, Id, Ident, Path, Span, TypeExpr};

/// A top-level declaration in a module.
pub enum Item {
    Func(Id<Func>),
    Struct(Id<Struct>),
    Use(Use),
    Extern(Extern),
    Impl(Impl),
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

/// `impl Point { fn sum(self: Point) -> i32 { ... } }`
pub struct Impl {
    /// The struct the methods are for: `Point`, or one from another module, `box2d::B2Vec2`.
    pub target: Path,
    pub methods: Vec<Id<Func>>,
}

/// A function, method, or extern declaration (which has no `body`).
pub struct Func {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub params: Vec<Param>,
    pub returns: Option<Id<TypeExpr>>,
    pub body: Option<Id<Expr>>,
    /// An extern function taking extra arguments after its parameters, like C's `printf`.
    pub variadic: bool,
    /// For an extern function, the symbol it links to when that isn't its name: som's
    /// `init_window` is raylib's `InitWindow`. Worked out from the library while checking.
    pub link_name: Option<Box<str>>,
}

pub struct Param {
    pub name: Ident,
    pub ty: Option<Id<TypeExpr>>,
    /// `out remainder: i32`: the function fills it in, and callers pass `out name`.
    /// Passed as a pointer, like `&mut i32`.
    pub out: bool,
}

pub struct Struct {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub name: Ident,
    pub fields: Vec<Field>,
    /// Values are copied rather than moved. Set for structs imported from a C header,
    /// which are plain data.
    pub copy: bool,
    /// `extern struct Sqlite3;`: a struct only C sees inside. Its values are handles,
    /// pointers to one, which som can pass around but not look into or create.
    pub handle: bool,
}

pub struct Field {
    /// The `///` comment above it.
    pub doc: Option<String>,
    pub name: Ident,
    pub ty: Id<TypeExpr>,
}
