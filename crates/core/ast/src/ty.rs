use crate::{Id, Ident, Path};

/// A type annotation as written. Resolving it to an actual type is the type checker's job.
pub enum TypeExpr {
    /// `()`
    Unit,
    /// `i32`, `bool`, `str`, ...
    Primitive(Primitive),
    /// `Point`, `geo::Point`
    Named(Path),
    /// `*`, an untyped raw pointer
    Pointer,
    /// `&T`, `&mut T`, `&'a T`
    Reference {
        mutable: bool,
        lifetime: Option<Ident>,
        to: Id<TypeExpr>,
    },
    /// `T?`: a `T`, or `none`.
    Optional(Id<TypeExpr>),
    /// `[T; N]`: `N` values of type `T`.
    Array { element: Id<TypeExpr>, len: u64 },
    /// `fn(A, B) -> C`
    Fn {
        params: Vec<Id<TypeExpr>>,
        returns: Option<Id<TypeExpr>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Primitive {
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
    F32,
    F64,
    Bool,
    Char,
    Str,
}
