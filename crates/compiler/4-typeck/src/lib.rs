//! Type checking by unification. `Typed` is a set of side tables keyed by AST ids, holding
//! no inference variables.

mod check;
mod consts;
mod error;
mod exhaustive;
mod impls;
pub mod stubs;
mod ty;
mod unify;

/// `impls`, by the name other crates know it by.
pub mod traits {
    pub use crate::impls::*;
}

pub use consts::{ConstValue, ConstValues, const_number, const_number_with};
pub use error::{NotForC, TypeError, Unused};
pub use ty::{FloatTy, IntTy, Ty, TypeId, TypeTable, Value};

use som_ast::{Ast, Expr, Func, Id, Pattern, Stmt, Struct};
use som_resolve::{DefId, Names};
use std::collections::{HashMap, HashSet};

pub fn check(ast: &Ast, names: &Names) -> (Typed, Vec<TypeError>) {
    let checked = check_with(ast, names, &ConstValues::default());
    (checked.typed, checked.errors)
}

/// A constant computed by running code that's needed here (an array length, an `@if`) but
/// isn't in `known` goes in `needed`: errors it causes aren't reported, and the program is
/// checked again once it's known.
pub fn check_with(ast: &Ast, names: &Names, known: &ConstValues) -> Checked {
    let mut checker = check::Checker::new(ast, names);
    checker.known = known.clone();
    let (typed, errors, needed) = checker.run_needing();
    Checked {
        typed,
        errors,
        needed,
    }
}

/// Checks one instance of generic `func`, its parameters bound to `args`. Types go into
/// `table` (lowering's) so its ids stay valid. Only errors inside `regions` are kept: the rest
/// of the body was checked once, generically.
pub fn check_instance(
    ast: &Ast,
    names: &Names,
    known: &ConstValues,
    table: TypeTable,
    func: Id<Func>,
    args: &[TypeId],
    regions: &[som_base::Span],
) -> (Typed, Vec<TypeError>) {
    let mut checker = check::Checker::with_table(ast, names, table);
    checker.known = known.clone();
    let (typed, errors) = checker.run_instance(func, args);
    let inside = |error: &TypeError| {
        let diagnostic = error.to_diagnostic();
        let Some(label) = diagnostic.labels.iter().find(|l| l.is_primary) else {
            return false;
        };
        let at = &label.span;
        regions.iter().any(|region| {
            region.source.identifier() == at.source.identifier()
                && region.start_offset <= at.start_offset
                && at.start_offset < region.start_offset + region.length
        })
    };
    let errors = errors.into_iter().filter(|e| inside(e)).collect();
    (typed, errors)
}

pub struct Checked {
    pub typed: Typed,
    pub errors: Vec<TypeError>,
    /// Constants that must be computed by running code first, in the order first needed.
    pub needed: Vec<Id<som_ast::Const>>,
}

/// Whether member `func`'s `@if`s hold for a `receiver` value, for the editor's completions.
/// `None` on a generic type's value.
pub fn member_holds(
    ast: &Ast,
    names: &Names,
    typed: &Typed,
    func: Id<Func>,
    receiver: TypeId,
) -> Option<bool> {
    if ast.funcs[func].when.is_empty() {
        return Some(true);
    }
    let mut checker = check::Checker::with_table(ast, names, typed.table.clone());
    checker.known = typed.const_values.clone();
    checker.member_holds_in(typed.impls.clone(), func, receiver)
}

#[derive(Clone, Debug)]
pub struct Signature {
    pub params: Vec<TypeId>,
    pub ret: TypeId,
}

pub struct Typed {
    pub table: TypeTable,
    exprs: Vec<TypeId>,
    locals: HashMap<DefId, TypeId>,
    funcs: Vec<Signature>,
    /// Values that become a struct → `From`'s `from` and its type arguments.
    conversions: HashMap<Id<Expr>, (DefId, Vec<TypeId>)>,
    /// Generic calls (`Call`, `MethodCall`, or a `Var` naming one) → their type arguments.
    type_args: HashMap<Id<Expr>, Vec<TypeId>>,
    /// References to arrays used where a slice is expected (`&numbers` for a `[i32]`), and
    /// `[mut T]`s used where a `[T]` is.
    sliced: HashMap<Id<Expr>, TypeId>,
    /// Every `impl` block, indexed like `Names::impls`.
    pub impls: Vec<impls::ImplInfo>,
    /// The constant values it was checked with.
    const_values: ConstValues,
    records: Records,
}

/// Holds no inference variables, so it moves into `Typed` as is.
#[derive(Default)]
pub(crate) struct Records {
    /// Field types of each struct; for an enum, every variant's fields, one after another.
    pub(crate) structs: Vec<Vec<TypeId>>,
    pub(crate) common: Vec<Vec<CommonField>>,
    pub(crate) patterns: HashMap<Id<Pattern>, PatternInfo>,
    pub(crate) receivers: HashMap<Id<Expr>, Receiver>,
    /// Calls and function values naming an overloaded function → the one they name.
    pub(crate) callees: HashMap<Id<Expr>, DefId>,
    pub(crate) trait_ops: HashMap<Id<Expr>, som_lang::LangTrait>,
    /// `match` arms that never finish, like `{ return 0; }`: they give no value.
    pub(crate) diverging: HashSet<Id<Expr>>,
    /// `[value; N]` → `N`, as an array type's length.
    pub(crate) counts: HashMap<Id<Expr>, TypeId>,
    pub(crate) methods: HashMap<Id<Expr>, DefId>,
    pub(crate) fields: HashMap<Id<Expr>, usize>,
    /// Expressions that become an optional where used: `5` in `let x: i32? = 5`.
    pub(crate) wrapped: HashSet<Id<Expr>>,
    /// `&mut T`s used where a `&T` is expected: each lent as `&*r`.
    pub(crate) downgraded: HashSet<Id<Expr>>,
    /// Reads of an optional place inside `if x { … }`: they read the value it holds.
    pub(crate) narrowed_reads: HashSet<Id<Expr>>,
    /// Operands read through the reference they are: `self * self` of a `&i32`.
    pub(crate) read_through: HashSet<Id<Expr>>,
    /// `#T`s read through to what they share: `node` in `node.value`, a `&T` from its `get`.
    pub(crate) shared: HashSet<Id<Expr>>,
    pub(crate) loops: HashMap<Id<Stmt>, Loop>,
    /// `@if`s → whether the condition holds: which branch there is.
    pub(crate) settled_ifs: HashMap<Id<Stmt>, bool>,
    /// `@if`s whose condition is a function's generic parameters: checked for each instance.
    pub(crate) regions: HashSet<Id<Stmt>>,
    /// The functions with such an `@if`.
    pub(crate) per_instance: HashSet<Id<Func>>,
    /// `@for f in T` over a generic type, in one instance: the copy for `T`'s struct.
    pub(crate) chosen_arms: HashMap<Id<Stmt>, Id<Expr>>,
    /// `match`es over a generic enum's variants: checked for each instance.
    pub(crate) match_regions: HashSet<Id<Expr>>,
    /// Those, in one instance: the copy for `T`'s enum.
    pub(crate) chosen_matches: HashMap<Id<Expr>, Id<Expr>>,
    pub(crate) array_methods: HashMap<Id<Expr>, ArrayMethod>,
    pub(crate) pointer_methods: HashMap<Id<Expr>, PointerMethod>,
}

/// A field every variant of an enum has, by the same name and type. It's reached without a
/// `match` and stored outside the variants, in the same place for all.
#[derive(Clone, Debug)]
pub struct CommonField {
    pub name: Box<str>,
    /// Where it is in each variant's fields, as an index into the enum's payload.
    pub payload: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receiver {
    /// `a.len()` for `fn len(self: &List)` is `(&a).len()`.
    Borrow { mutable: bool },
    /// `r.area()` for `fn area(self: Point)`, with `r` a `&Point`, is `(*r).area()`.
    Deref,
    /// A `&mut self` lent to a `&self` method: `(&*self).get()`.
    Share,
}

#[derive(Clone, Debug, Default)]
pub struct PatternInfo {
    /// Matched against an optional, it matches the value held: `Circle(r)` against a `Shape?`.
    pub unwraps: bool,
    /// For a variant pattern or a bare name naming one.
    pub variant: Option<usize>,
    /// For a variant pattern: the field each of its patterns matches, in the order written.
    pub fields: Vec<usize>,
    /// A binding matched through a reference borrows instead of copying or moving: whether
    /// as `&mut`.
    pub by_ref: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loop {
    Range,
    Elements {
        take: Take,
    },
    /// Until `next` gives `none`. With `copied`, the iterator gives references to values that
    /// copy, and the loop takes copies.
    Iterator {
        start: Start,
        copied: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Take {
    /// Of an array it owns whose elements copy, or of one it borrows.
    Copy,
    /// From `&array` or a slice, to elements that move rather than copy.
    Ref,
    /// From `&mut array` or a `&mut [T]`.
    RefMut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// `for x in list.iter()`.
    Itself,
    /// `for x in &value`: `Iterable::iter` makes it.
    Iter,
    /// `for x in &mut value`: `IterableMut::iter_mut` makes it.
    IterMut,
    /// `for x in value`: `IntoIterator::into_iter` makes it, from the value given up.
    IntoIter,
}

som_lang::named_enum! {
    pub enum ArrayMethod {
        Len = "len",
        /// A copy of the element, or `none` past the end.
        Get = "get",
        GetRef = "get_ref",
        GetMut = "get_mut",
        /// Returns whether `i` was in bounds.
        Set = "set",
        /// A `str`'s bytes, as a `[u8]`. Only text has it.
        Bytes = "bytes",
        /// `none` if the range isn't within the elements. Text gives text.
        Slice = "slice",
        SliceMut = "slice_mut",
    }
}

som_lang::named_enum! {
    /// Raw memory operations that nothing checks, for building types like `List` in som.
    pub enum PointerMethod {
        /// `p` becomes room for `count` `T`s, keeping what fits. A null `p` starts empty.
        /// Copies of the old pointer aren't valid any more.
        Resize = "resize",
        Free = "free",
        /// Copied or moved out.
        Read = "read",
        /// Without reading or freeing what was there.
        Write = "write",
        At = "at",
        AtMut = "at_mut",
        View = "view",
        ViewMut = "view_mut",
        IsNull = "is_null",
    }
}

impl PointerMethod {
    /// Arguments after the pointer.
    pub fn arity(self) -> usize {
        match self {
            PointerMethod::Free | PointerMethod::IsNull => 0,
            PointerMethod::Write => 2,
            _ => 1,
        }
    }
}

impl ArrayMethod {
    pub(crate) fn text_only(self) -> bool {
        self == ArrayMethod::Bytes
    }

    /// Read as a property, `items.len`, rather than called.
    pub(crate) fn is_prop(self) -> bool {
        self == ArrayMethod::Len
    }

    pub(crate) fn method_names(text: bool) -> Vec<String> {
        (ArrayMethod::ALL.iter())
            .filter(|m| !m.is_prop() && (text || !m.text_only()))
            .map(|m| m.name().to_string())
            .collect()
    }

    /// Arguments after the array.
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

impl som_ast::Settled for Typed {
    fn settled_if(&self, stmt: Id<Stmt>) -> Option<bool> {
        Typed::settled_if(self, stmt)
    }

    fn chosen_match(&self, expr: Id<Expr>) -> Option<Id<Expr>> {
        Typed::chosen_match(self, expr)
    }
}

impl Typed {
    pub fn ty(&self, id: TypeId) -> &Ty {
        self.table.get(id)
    }

    pub fn expr(&self, expr: Id<Expr>) -> TypeId {
        self.exprs[expr.id]
    }

    /// False in an `@if` checked for each instance, or where checking failed.
    pub fn is_checked_expr(&self, expr: Id<Expr>) -> bool {
        !matches!(self.ty(self.expr(expr)), Ty::Error)
    }

    pub fn local(&self, def: DefId) -> TypeId {
        self.locals[&def]
    }

    /// `None` for a definition never checked, as in code with errors.
    pub fn try_local(&self, def: DefId) -> Option<TypeId> {
        self.locals.get(&def).copied()
    }

    pub fn func(&self, func: Id<Func>) -> &Signature {
        &self.funcs[func.id]
    }

    /// For an enum, its payload: every variant's fields, one after another.
    pub fn fields(&self, s: Id<Struct>) -> &[TypeId] {
        &self.records.structs[s.id]
    }

    /// In the order the first variant declares them. A `Field` on an enum reads the one at its
    /// `field` index.
    pub fn common_fields(&self, s: Id<Struct>) -> &[CommonField] {
        &self.records.common[s.id]
    }

    pub fn is_common(&self, s: Id<Struct>, index: usize) -> bool {
        self.records.common[s.id]
            .iter()
            .any(|c| c.payload.contains(&index))
    }

    /// For an overloaded name, the one whose types fit; otherwise what the resolver found.
    pub fn callee(&self, expr: Id<Expr>, names: &Names) -> Option<DefId> {
        self.records
            .callees
            .get(&expr)
            .copied()
            .or_else(|| names.expr(expr))
    }

    /// `Eq` for `==` on a struct implementing it, `Add` for `+`.
    pub fn operator_trait(&self, expr: Id<Expr>) -> Option<som_lang::LangTrait> {
        self.records.trait_ops.get(&expr).copied()
    }

    /// `From`'s `from`, which `expr` goes through to become the struct expected where it's
    /// used, with its type arguments: the struct, then `expr`'s type.
    pub fn conversion(&self, expr: Id<Expr>) -> Option<(DefId, &[TypeId])> {
        self.conversions
            .get(&expr)
            .map(|(d, args)| (*d, args.as_slice()))
    }

    pub fn receiver(&self, expr: Id<Expr>) -> Option<Receiver> {
        self.records.receivers.get(&expr).copied()
    }

    pub fn pattern(&self, pattern: Id<Pattern>) -> Option<&PatternInfo> {
        self.records.patterns.get(&pattern)
    }

    pub fn diverges(&self, expr: Id<Expr>) -> bool {
        self.records.diverging.contains(&expr)
    }

    /// `N` of `[value; N]`, in terms of the enclosing function's value parameters if any.
    pub fn count(&self, expr: Id<Expr>) -> TypeId {
        self.records.counts[&expr]
    }

    pub fn method(&self, expr: Id<Expr>) -> Option<DefId> {
        self.records.methods.get(&expr).copied()
    }

    pub fn field(&self, expr: Id<Expr>) -> Option<usize> {
        self.records.fields.get(&expr).copied()
    }

    pub fn type_args(&self, expr: Id<Expr>) -> &[TypeId] {
        self.type_args.get(&expr).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The slice type `expr` becomes where it's used, if it does: a reference to an array, or
    /// a `[mut T]` where a `[T]` is expected.
    pub fn sliced_to(&self, expr: Id<Expr>) -> Option<TypeId> {
        self.sliced.get(&expr).copied()
    }

    /// Whether `@if` `stmt` is checked for each instance of its function rather than here.
    pub fn is_region(&self, stmt: Id<Stmt>) -> bool {
        self.records.regions.contains(&stmt)
    }

    /// What's checked per instance: `@if`s on generic parameters and members only some
    /// instances have. Errors found checking an instance are kept only inside these.
    pub fn region_spans(&self, ast: &Ast) -> Vec<som_base::Span> {
        let regions = self
            .records
            .regions
            .iter()
            .map(|&stmt| ast.stmts.span(stmt).clone());
        let members = (self.records.per_instance.iter())
            .filter(|&&func| !ast.funcs[func].when.is_empty())
            .map(|&func| ast.funcs.span(func).clone());
        // A function's end, where one not returning on every path is said: whether it does
        // depends on which way its `@if`s go.
        let ends = (self.records.per_instance.iter())
            .filter_map(|&func| ast.funcs[func].body)
            .map(|body| ast.exprs.span(body).last_char());
        let matches = self
            .records
            .match_regions
            .iter()
            .map(|&m| ast.exprs.span(m).clone());
        regions.chain(members).chain(ends).chain(matches).collect()
    }

    /// A `match` over a generic enum with `@for (i, k) in T` arms, in one instance: the copy
    /// written out for `T`'s enum.
    pub fn chosen_match(&self, expr: Id<Expr>) -> Option<Id<Expr>> {
        self.records.chosen_matches.get(&expr).copied()
    }

    pub fn regions(&self) -> impl Iterator<Item = Id<Stmt>> + '_ {
        self.records.regions.iter().copied()
    }

    /// `@for f in T` over a generic type, in one instance: the copy written out for `T`'s
    /// struct, a block.
    pub fn chosen_arm(&self, stmt: Id<Stmt>) -> Option<Id<Expr>> {
        self.records.chosen_arms.get(&stmt).copied()
    }

    pub fn is_per_instance(&self, func: Id<Func>) -> bool {
        self.records.per_instance.contains(&func)
    }

    pub fn const_values(&self) -> &ConstValues {
        &self.const_values
    }

    pub fn const_value(&self, c: Id<som_ast::Const>) -> Option<ConstValue> {
        self.const_values.get(c)
    }

    pub fn const_number(&self, ast: &Ast, names: &Names, expr: Id<Expr>) -> Option<i128> {
        const_number_with(ast, names, &self.const_values, expr)
    }

    /// `true` for an `@if`'s first branch, `false` for its `else`; `None` for an `if` decided
    /// at run time.
    pub fn settled_if(&self, stmt: Id<Stmt>) -> Option<bool> {
        self.records.settled_ifs.get(&stmt).copied()
    }

    pub fn for_loop(&self, stmt: Id<Stmt>) -> Option<Loop> {
        self.records.loops.get(&stmt).copied()
    }

    pub fn array_method(&self, expr: Id<Expr>) -> Option<ArrayMethod> {
        self.records.array_methods.get(&expr).copied()
    }

    pub fn pointer_method(&self, expr: Id<Expr>) -> Option<PointerMethod> {
        self.records.pointer_methods.get(&expr).copied()
    }

    /// Reads the value a narrowed optional holds, rather than the optional.
    pub fn is_narrowed(&self, expr: Id<Expr>) -> bool {
        self.records.narrowed_reads.contains(&expr)
    }

    /// Operand `expr`, a reference to a number or `bool`, is read through: `self * self` of a
    /// `&i32` multiplies the numbers.
    pub fn is_read_through(&self, expr: Id<Expr>) -> bool {
        self.records.read_through.contains(&expr)
    }

    /// `expr`, a `#T`, is read through to what it shares (`node` in `node.value`), as the
    /// `&T` its `get` gives.
    pub fn is_shared_read(&self, expr: Id<Expr>) -> bool {
        self.records.shared.contains(&expr)
    }

    /// A `&mut T` used where a `&T` is expected: lent as `&*expr`, so it's still usable after.
    pub fn is_downgraded(&self, expr: Id<Expr>) -> bool {
        self.records.downgraded.contains(&expr)
    }

    pub fn is_wrapped(&self, expr: Id<Expr>) -> bool {
        self.records.wrapped.contains(&expr)
    }

    pub fn display(&self, id: TypeId, names: &Names) -> String {
        self.table.display(id, names)
    }
}
