//! Type checking: infers the type of every expression and checks that they fit.
//!
//! Types are inferred by unification as the checker walks each function, so
//! a value's type is known as soon as anything constrains it. Integer and float
//! literals start as "some integer"/"some float" and default to `i32`/`f64`.
//!
//! The result, `Typed`, is a set of side tables keyed by AST ids, like `Names`.
//! It contains no inference variables.

mod check;
mod error;
mod ty;
mod unify;

pub use error::TypeError;
pub use ty::{FloatTy, IntTy, Ty, TypeId, TypeTable, Value};

use som_ast::{Ast, Expr, Func, Id, Stmt, Struct};
use som_resolve::{DefId, Names};
use std::collections::{HashMap, HashSet};

pub fn check(ast: &Ast, names: &Names) -> (Typed, Vec<TypeError>) {
    check::Checker::new(ast, names).run()
}

/// A function's parameter and return types.
#[derive(Clone, Debug)]
pub struct Signature {
    pub params: Vec<TypeId>,
    pub ret: TypeId,
}

/// The types of everything in a program.
pub struct Typed {
    pub table: TypeTable,
    exprs: Vec<TypeId>,
    /// Parameters and `let` locals.
    locals: HashMap<DefId, TypeId>,
    funcs: Vec<Signature>,
    /// Field types of each struct, in declaration order.
    structs: Vec<Vec<TypeId>>,
    /// `[value; N]` → `N`, as an array type's length.
    counts: HashMap<Id<Expr>, TypeId>,
    /// `MethodCall` → the method it calls.
    methods: HashMap<Id<Expr>, DefId>,
    /// `Field` → the index of the field it reads.
    fields: HashMap<Id<Expr>, usize>,
    /// Calls to generic functions (`Call`, `MethodCall`, or `Var` naming one) → the
    /// types their type parameters were instantiated with.
    type_args: HashMap<Id<Expr>, Vec<TypeId>>,
    /// Expressions whose value becomes an optional's where it's used: `5` in
    /// `let x: i32? = 5`.
    wrapped: HashSet<Id<Expr>>,
    /// Reads of an optional place inside `if x { … }`: they read the value it holds.
    narrowed_reads: HashSet<Id<Expr>>,
    /// References to arrays used where a slice is expected (`&numbers` for a `[i32]`), and
    /// `[mut T]`s used where a `[T]` is.
    sliced: HashMap<Id<Expr>, TypeId>,
    /// `for` loops → what they loop over.
    loops: HashMap<Id<Stmt>, Loop>,
    /// Method calls on arrays → the built-in method they call.
    array_methods: HashMap<Id<Expr>, ArrayMethod>,
}

/// What a `for` loop goes through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loop {
    /// Each integer in a range.
    Range,
    /// Each element of an array or slice: a copy of it, or, for elements that move rather
    /// than copy, a reference to it.
    Elements { by_reference: bool },
}

/// A method every array and slice has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayMethod {
    /// `a.len()`: how many elements it has.
    Len,
    /// `a.get(i)`: a copy of the element at `i`, or `none` past the end.
    Get,
    /// `a.get_ref(i)`: a reference to the element at `i`, or `none`.
    GetRef,
    /// `a.get_mut(i)`: a mutable reference to the element at `i`, or `none`.
    GetMut,
    /// `a.set(i, v)`: stores `v` at `i`, returning whether `i` was in bounds.
    Set,
    /// `s.bytes()`: a `str`'s bytes, as a `[u8]`. Only text has it.
    Bytes,
    /// `a.slice(r)`: a view of the elements in the range `r`, or `none` if it isn't within
    /// them. Text gives text.
    Slice,
    /// `a.slice_mut(r)`: the same, as a `[mut T]`.
    SliceMut,
}

impl ArrayMethod {
    pub fn from_name(name: &str) -> Option<ArrayMethod> {
        Some(match name {
            "len" => ArrayMethod::Len,
            "get" => ArrayMethod::Get,
            "get_ref" => ArrayMethod::GetRef,
            "get_mut" => ArrayMethod::GetMut,
            "set" => ArrayMethod::Set,
            "bytes" => ArrayMethod::Bytes,
            "slice" => ArrayMethod::Slice,
            "slice_mut" => ArrayMethod::SliceMut,
            _ => return None,
        })
    }

    pub const ALL: [(&str, ArrayMethod); 8] = [
        ("len", ArrayMethod::Len),
        ("get", ArrayMethod::Get),
        ("get_ref", ArrayMethod::GetRef),
        ("get_mut", ArrayMethod::GetMut),
        ("set", ArrayMethod::Set),
        ("bytes", ArrayMethod::Bytes),
        ("slice", ArrayMethod::Slice),
        ("slice_mut", ArrayMethod::SliceMut),
    ];

    /// How many arguments it takes, after the array.
    pub fn arity(self) -> usize {
        match self {
            ArrayMethod::Len | ArrayMethod::Bytes => 0,
            ArrayMethod::Get
            | ArrayMethod::GetRef
            | ArrayMethod::GetMut
            | ArrayMethod::Slice
            | ArrayMethod::SliceMut => 1,
            ArrayMethod::Set => 2,
        }
    }
}

impl Typed {
    pub fn ty(&self, id: TypeId) -> &Ty {
        self.table.get(id)
    }

    pub fn expr(&self, expr: Id<Expr>) -> TypeId {
        self.exprs[expr.id]
    }

    /// The type of a parameter or local.
    pub fn local(&self, def: DefId) -> TypeId {
        self.locals[&def]
    }

    /// Like `local`, but `None` for a definition that was never checked, as in code with errors.
    pub fn try_local(&self, def: DefId) -> Option<TypeId> {
        self.locals.get(&def).copied()
    }

    pub fn func(&self, func: Id<Func>) -> &Signature {
        &self.funcs[func.id]
    }

    pub fn fields(&self, s: Id<Struct>) -> &[TypeId] {
        &self.structs[s.id]
    }

    /// How many copies `[value; N]` makes: `N`, as an array type's length. It's in terms of
    /// the enclosing function's value parameters, if it has any.
    pub fn count(&self, expr: Id<Expr>) -> TypeId {
        self.counts[&expr]
    }

    /// The method a `MethodCall` expression calls.
    pub fn method(&self, expr: Id<Expr>) -> Option<DefId> {
        self.methods.get(&expr).copied()
    }

    /// The field index a `Field` expression reads.
    pub fn field(&self, expr: Id<Expr>) -> Option<usize> {
        self.fields.get(&expr).copied()
    }

    /// The type arguments of a call to a generic function. Empty if it isn't generic.
    pub fn type_args(&self, expr: Id<Expr>) -> &[TypeId] {
        self.type_args.get(&expr).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The slice type `expr` becomes where it's used, if it does: a reference to an array, or
    /// a `[mut T]` where a `[T]` is expected.
    pub fn sliced_to(&self, expr: Id<Expr>) -> Option<TypeId> {
        self.sliced.get(&expr).copied()
    }

    /// What the `for` loop `stmt` goes through.
    pub fn for_loop(&self, stmt: Id<Stmt>) -> Option<Loop> {
        self.loops.get(&stmt).copied()
    }

    /// The built-in method a method call on an array calls.
    pub fn array_method(&self, expr: Id<Expr>) -> Option<ArrayMethod> {
        self.array_methods.get(&expr).copied()
    }

    /// Whether `expr` reads the value a narrowed optional holds, rather than the optional.
    pub fn is_narrowed(&self, expr: Id<Expr>) -> bool {
        self.narrowed_reads.contains(&expr)
    }

    /// Whether `expr`'s value is wrapped in an optional where it's used.
    pub fn is_wrapped(&self, expr: Id<Expr>) -> bool {
        self.wrapped.contains(&expr)
    }

    pub fn display(&self, id: TypeId, names: &Names) -> String {
        self.table.display(id, names)
    }
}
