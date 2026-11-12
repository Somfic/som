use crate::{Expr, Id, Ident, Path, TypeExpr};

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
    pub funcs: Vec<Id<Func>>,
}

/// `impl Point { fn sum(self: Point) -> i32 { ... } }`
pub struct Impl {
    pub target: Ident,
    pub methods: Vec<Id<Func>>,
}

/// A function, method, or extern declaration (which has no `body`).
pub struct Func {
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub params: Vec<Param>,
    pub returns: Option<Id<TypeExpr>>,
    pub body: Option<Id<Expr>>,
}

pub struct Param {
    pub name: Ident,
    pub ty: Option<Id<TypeExpr>>,
}

pub struct Struct {
    pub name: Ident,
    pub fields: Vec<Field>,
}

pub struct Field {
    pub name: Ident,
    pub ty: Id<TypeExpr>,
}
