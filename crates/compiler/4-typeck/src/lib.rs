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
mod exhaustive;
mod ty;
mod unify;

pub use error::TypeError;
pub use ty::{FloatTy, IntTy, Ty, TypeId, TypeTable, Value};

use som_ast::{Ast, Expr, Func, Id, Pattern, Stmt, Struct};
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
    /// Field types of each struct, in declaration order; for an enum, every variant's fields,
    /// one variant after another.
    structs: Vec<Vec<TypeId>>,
    /// Each enum's common fields: see `CommonField`.
    common: Vec<Vec<CommonField>>,
    /// What each `match` arm's pattern turned out to be.
    patterns: HashMap<Id<Pattern>, PatternInfo>,
    /// Method receivers passed other than as they are: see `Receiver`.
    receivers: HashMap<Id<Expr>, Receiver>,
    /// Calls and function values naming a function that shares its name with others → the
    /// one they name.
    callees: HashMap<Id<Expr>, DefId>,
    /// `match` arms that never finish, like `{ return 0; }`: they have no value to give.
    diverging: HashSet<Id<Expr>>,
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
    /// Method calls on typed pointers → the built-in method they call.
    pointer_methods: HashMap<Id<Expr>, PointerMethod>,
}

/// A field every variant of an enum has, by the same name and of the same type: `base` in
/// `enum Test<T> { A { base: i32, a: T }, B { base: i32, b: T } }`. It can be reached
/// without a `match`, `t.base`, and lives outside the variants, in the same place for all.
#[derive(Clone, Debug)]
pub struct CommonField {
    pub name: Box<str>,
    /// Where it is in each variant's fields, as an index into the enum's payload.
    pub payload: Vec<usize>,
}

/// How a method call's receiver becomes the method's `self`, when it isn't as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receiver {
    /// Borrowed: `a.len()` for `fn len(self: &List)` is `(&a).len()`, and with `mutable`,
    /// `(&mut a).push(1)`.
    Borrow { mutable: bool },
    /// Copied out of the reference it is: `r.area()` for `fn area(self: Point)`, with `r` a
    /// `&Point`, is `(*r).area()`.
    Deref,
}

/// What a pattern in a `match` arm turned out to be, once the type it matches was known.
#[derive(Clone, Debug, Default)]
pub struct PatternInfo {
    /// It's matched against an optional, and matches the value the optional holds: `5` or
    /// `Circle(r)` against a `Shape?`.
    pub unwraps: bool,
    /// A variant pattern, or a bare name that's a variant: which of the enum's variants.
    pub variant: Option<usize>,
    /// For a variant pattern: the field each of its patterns matches, in the order written.
    pub fields: Vec<usize>,
    /// A binding that borrows what it matches instead of copying or moving it, matched through
    /// a reference: whether it's `&mut`.
    pub by_ref: Option<bool>,
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

/// A method every typed pointer `*T` has: raw memory operations that nothing checks, for
/// building types like `List` in som.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerMethod {
    /// `p.resize(count)`: room for `count` `T`s, holding what `p` held (as many as fit); `p`
    /// itself is no longer valid. A null `p` starts empty.
    Resize,
    /// `p.free()`: gives the memory back.
    Free,
    /// `p.read(i)`: the `T` at `i`, copied or moved out.
    Read,
    /// `p.write(i, value)`: stores `value` at `i`, without reading or freeing what was there.
    Write,
    /// `p.at(i)`: a reference to the `T` at `i`.
    At,
    /// `p.at_mut(i)`: a `&mut` reference to the `T` at `i`.
    AtMut,
    /// `p.view(len)`: the `len` `T`s starting at `p`, as a `[T]`.
    View,
    /// `p.view_mut(len)`: as a `[mut T]`.
    ViewMut,
    /// `p.is_null()`
    IsNull,
}

impl PointerMethod {
    pub const ALL: [(&str, PointerMethod); 9] = [
        ("resize", PointerMethod::Resize),
        ("free", PointerMethod::Free),
        ("read", PointerMethod::Read),
        ("write", PointerMethod::Write),
        ("at", PointerMethod::At),
        ("at_mut", PointerMethod::AtMut),
        ("view", PointerMethod::View),
        ("view_mut", PointerMethod::ViewMut),
        ("is_null", PointerMethod::IsNull),
    ];

    pub fn from_name(name: &str) -> Option<PointerMethod> {
        Self::ALL.iter().find(|(n, _)| *n == name).map(|&(_, m)| m)
    }

    /// How many arguments it takes, after the pointer.
    pub fn arity(self) -> usize {
        match self {
            PointerMethod::Free | PointerMethod::IsNull => 0,
            PointerMethod::Write => 2,
            _ => 1,
        }
    }
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

    /// A struct's field types, or an enum's payload: every variant's fields, one after another.
    pub fn fields(&self, s: Id<Struct>) -> &[TypeId] {
        &self.structs[s.id]
    }

    /// An enum's common fields, in the order its first variant declares them. A `Field`
    /// expression on an enum reads the one at its `field` index.
    pub fn common_fields(&self, s: Id<Struct>) -> &[CommonField] {
        &self.common[s.id]
    }

    /// Whether payload field `index` of enum `s` is one of its common fields.
    pub fn is_common(&self, s: Id<Struct>, index: usize) -> bool {
        self.common[s.id].iter().any(|c| c.payload.contains(&index))
    }

    /// The function call or function value `expr` names: the one of those sharing its name
    /// whose types fit, or what the resolver found if it's alone.
    pub fn callee(&self, expr: Id<Expr>, names: &Names) -> Option<DefId> {
        self.callees
            .get(&expr)
            .copied()
            .or_else(|| names.expr(expr))
    }

    /// How method call receiver `expr` becomes the method's `self`, if not as it is.
    pub fn receiver(&self, expr: Id<Expr>) -> Option<Receiver> {
        self.receivers.get(&expr).copied()
    }

    /// What a `match` arm's pattern turned out to be.
    pub fn pattern(&self, pattern: Id<Pattern>) -> Option<&PatternInfo> {
        self.patterns.get(&pattern)
    }

    /// Whether `match` arm body `expr` never finishes, so it has no value.
    pub fn diverges(&self, expr: Id<Expr>) -> bool {
        self.diverging.contains(&expr)
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

    /// The built-in method a method call on a typed pointer calls.
    pub fn pointer_method(&self, expr: Id<Expr>) -> Option<PointerMethod> {
        self.pointer_methods.get(&expr).copied()
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
