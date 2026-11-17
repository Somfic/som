use crate::{BinOp, Id, Ident, Path};

pub enum TypeExpr {
    Unit,
    Primitive(Primitive),
    Named {
        path: Path,
        args: Vec<Id<TypeExpr>>,
    },
    Pointer,
    /// `*T`: unchecked; what `List` is built on.
    PointerTo(Id<TypeExpr>),
    Reference {
        mutable: bool,
        lifetime: Option<Ident>,
        to: Id<TypeExpr>,
    },
    Optional(Id<TypeExpr>),
    /// `#T`: the standard library's `Rc<T>`.
    Shared(Id<TypeExpr>),
    Array {
        element: Id<TypeExpr>,
        len: Id<TypeExpr>,
    },
    /// `64` in `Buffer<64>` or `[u8; 64]`.
    Int(u128),
    /// `true` in `Socket<true>`.
    Bool(bool),
    /// `A + B` in `[T; A + B]`. `op` is `+` or `*`.
    Binary {
        op: BinOp,
        lhs: Id<TypeExpr>,
        rhs: Id<TypeExpr>,
    },
    /// `&[T]` or `&mut [T]`.
    Slice {
        element: Id<TypeExpr>,
        mutable: bool,
    },
    /// `[T]`, only what an `impl` is for: `impl Index<usize> for [T]`, whose `&self` is the
    /// slice.
    Elements(Id<TypeExpr>),
    Fn {
        params: Vec<Id<TypeExpr>>,
        returns: Option<Id<TypeExpr>>,
    },
}

pub use som_lang::{FloatTy, IntTy, Primitive};
