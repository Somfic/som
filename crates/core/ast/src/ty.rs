use crate::{BinOp, Id, Ident, Path};

/// A type annotation as written. Resolving it to an actual type is the type checker's job.
pub enum TypeExpr {
    /// `()`
    Unit,
    /// `i32`, `bool`, `str`, ...
    Primitive(Primitive),
    /// `Point`, `geo::Point`, `Pair<i32>`
    Named {
        path: Path,
        /// The arguments of a generic struct: `i32` in `Pair<i32>`, `64` in `Buffer<64>`.
        args: Vec<Id<TypeExpr>>,
    },
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
    /// `[T; N]`: `N` values of type `T`. `N` is a number or a value parameter.
    Array {
        element: Id<TypeExpr>,
        len: Id<TypeExpr>,
    },
    /// A number written where a type argument goes: `64` in `Buffer<64>` or `[u8; 64]`.
    Int(u128),
    /// `true` or `false` written where a type argument goes: `Socket<true>`.
    Bool(bool),
    /// Arithmetic on values where a type argument goes: `A + B` in `[T; A + B]`. `op` is
    /// `+` or `*`.
    Binary {
        op: BinOp,
        lhs: Id<TypeExpr>,
        rhs: Id<TypeExpr>,
    },
    /// `[T]` or `[mut T]`: a view of some number of values of type `T` that live elsewhere,
    /// known at run time. It borrows them; `mut` can change them.
    Slice {
        element: Id<TypeExpr>,
        mutable: bool,
    },
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
    /// `str`: text, a view of bytes that live elsewhere.
    Str,
    /// `cstr`: a pointer to text ending in a zero byte, for C.
    CStr,
}
