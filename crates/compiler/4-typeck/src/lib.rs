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
pub mod stubs;
pub mod traits;
mod ty;
mod unify;

pub use error::{NotForC, TypeError, Unused};
pub use ty::{FloatTy, IntTy, Ty, TypeId, TypeTable, Value};

use som_ast::{Ast, BinOp, Expr, Func, Id, Pattern, Stmt, Struct};
use som_resolve::{DefId, DefKind, Names};
use std::collections::{HashMap, HashSet};

pub fn check(ast: &Ast, names: &Names) -> (Typed, Vec<TypeError>) {
    let checked = check_with(ast, names, &ConstValues::default());
    (checked.typed, checked.errors)
}

/// Checks the program with the values of constants worked out so far, `known`. A constant
/// that's worked out by running code, needed where its value has to be known (an array's
/// length, an `@if`) and not in `known`, is `needed`: what's made of it this time round is
/// an error that isn't said, and the program is checked again once it's worked out.
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

/// Checks one instance of generic function `func`, its generic parameters standing for
/// `args` (`N` for `Ty::Value(3)`): its body, where an `@if N >= 3` is known. The types it
/// finds are added to `table` (lowering's), so ids already in it stay theirs. Only errors
/// inside `regions` (the `@if`s checked for each instance) are given: the rest of the body
/// was checked once, generically.
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

/// What checking a program gives, with the constants it still needs worked out.
pub struct Checked {
    pub typed: Typed,
    pub errors: Vec<TypeError>,
    /// Constants worked out by running code that are needed before the program can be
    /// checked, in the order they were first needed.
    pub needed: Vec<Id<som_ast::Const>>,
}

/// A constant's value, worked out by running it: what the type checker can use of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConstValue {
    Int(i128),
    Bool(bool),
    /// Anything else: a struct, text. Not a count, nor a condition.
    Other,
}

/// The values of constants worked out by running them.
#[derive(Clone, Debug, Default)]
pub struct ConstValues(pub HashMap<Id<som_ast::Const>, ConstValue>);

impl ConstValues {
    pub fn get(&self, c: Id<som_ast::Const>) -> Option<ConstValue> {
        self.0.get(&c).copied()
    }
}

/// Whether member `func`'s `@if`s (`@if N >= 3 { prop z … }`) hold on a value of type
/// `receiver`, once the program is checked: for the editor, which only offers the members a
/// value has. `None` where that isn't known, on a generic type's value.
pub fn member_holds(
    ast: &Ast,
    names: &Names,
    typed: &Typed,
    func: Id<Func>,
    receiver: TypeId,
) -> Option<bool> {
    let when = &ast.funcs[func].when;
    if when.is_empty() {
        return Some(true);
    }
    // The block's parameters, as they are in `receiver`: `N` is 3 in a `Row<3>`.
    let mut matched: HashMap<DefId, TypeId> = HashMap::new();
    if let Some(index) = names.impl_of(func)
        && let Some(target) = typed.impls[index].target
    {
        fn walk(
            typed: &Typed,
            pattern: TypeId,
            found: TypeId,
            params: &[DefId],
            matched: &mut HashMap<DefId, TypeId>,
        ) {
            if let Ty::Param(p) = typed.ty(pattern)
                && params.contains(p)
            {
                matched.insert(*p, found);
                return;
            }
            for (a, b) in typed
                .ty(pattern)
                .zip_children(typed.ty(found))
                .unwrap_or_default()
            {
                walk(typed, a, b, params, matched);
            }
        }
        let found = match *typed.ty(receiver) {
            Ty::Ref { to, .. } => to,
            _ => receiver,
        };
        walk(
            typed,
            target,
            found,
            &typed.impls[index].params,
            &mut matched,
        );
    }
    let checker = MemberConditions {
        ast,
        names,
        typed,
        matched,
    };
    let mut holds = Some(true);
    for &condition in when {
        match checker.holds(condition) {
            Some(true) => {}
            Some(false) => return Some(false),
            None => holds = None,
        }
    }
    holds
}

/// Works out a member's conditions with its block's parameters `matched` to a type's.
struct MemberConditions<'a> {
    ast: &'a Ast,
    names: &'a Names,
    typed: &'a Typed,
    matched: HashMap<DefId, TypeId>,
}

/// One side of `T == f32`.
#[derive(PartialEq)]
enum Operand {
    Type(TypeId),
    /// A struct named without its type arguments: `Pair`, any `Pair`.
    Struct(DefId),
}

impl MemberConditions<'_> {
    fn holds(&self, expr: Id<Expr>) -> Option<bool> {
        match &self.ast.exprs[expr] {
            Expr::Bool(b) => Some(*b),
            Expr::Not(inner) => self.holds(*inner).map(|b| !b),
            Expr::Binary { op, lhs, rhs } => match op {
                BinOp::And => Some(self.holds(*lhs)? && self.holds(*rhs)?),
                BinOp::Or => Some(self.holds(*lhs)? || self.holds(*rhs)?),
                BinOp::Equals | BinOp::NotEquals
                    if let (Some(a), Some(b)) = (self.operand(*lhs), self.operand(*rhs)) =>
                {
                    let same = match (a, b) {
                        (Operand::Struct(x), Operand::Type(t))
                        | (Operand::Type(t), Operand::Struct(x)) => {
                            matches!(self.typed.ty(t), Ty::Struct(y, _) if *y == x)
                        }
                        (a, b) => a == b,
                    };
                    Some(same == (*op == BinOp::Equals))
                }
                _ => {
                    let (a, b) = (self.number(*lhs)?, self.number(*rhs)?);
                    match op {
                        BinOp::Equals => Some(a == b),
                        BinOp::NotEquals => Some(a != b),
                        BinOp::LessThan => Some(a < b),
                        BinOp::GreaterThan => Some(a > b),
                        BinOp::LessThanOrEqual => Some(a <= b),
                        BinOp::GreaterThanOrEqual => Some(a >= b),
                        _ => None,
                    }
                }
            },
            _ => None,
        }
    }

    fn number(&self, expr: Id<Expr>) -> Option<i128> {
        if let Some(n) = self.typed.const_number(self.ast, self.names, expr) {
            return Some(n);
        }
        match &self.ast.exprs[expr] {
            Expr::Var(_) => match self.typed.ty(*self.matched.get(&self.names.expr(expr)?)?) {
                Ty::Value(Value::Int(n)) => Some(*n as i128),
                _ => None,
            },
            Expr::Binary { op, lhs, rhs } => {
                let (a, b) = (self.number(*lhs)?, self.number(*rhs)?);
                match op {
                    BinOp::Add => a.checked_add(b),
                    BinOp::Subtract => a.checked_sub(b),
                    BinOp::Multiply => a.checked_mul(b),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The type `expr` names, if it's one known here.
    fn operand(&self, expr: Id<Expr>) -> Option<Operand> {
        let ty = match &self.ast.exprs[expr] {
            Expr::Type(_) => self.typed.expr(expr),
            Expr::Var(_) => {
                let def = self.names.expr(expr)?;
                match self.names.def(def).kind {
                    DefKind::Struct(_) => return Some(Operand::Struct(def)),
                    DefKind::ImplParam { value: false, .. } => *self.matched.get(&def)?,
                    _ => return None,
                }
            }
            _ => return None,
        };
        self.settled(ty).then_some(Operand::Type(ty))
    }

    fn settled(&self, ty: TypeId) -> bool {
        match self.typed.ty(ty) {
            Ty::Var(_) | Ty::Param(_) | Ty::Error => false,
            t => t.children().into_iter().all(|c| self.settled(c)),
        }
    }
}

/// The integer a constant's value works out to: `W * H` of `@const W = 60;` and
/// `@const H = 20;` is 1200. `None` if it isn't one, or doesn't fit.
pub fn const_number(ast: &Ast, names: &Names, expr: Id<Expr>) -> Option<i128> {
    const_number_with(ast, names, &ConstValues::default(), expr)
}

/// Like `const_number`, with the values of constants worked out by running them, `known`.
pub fn const_number_with(
    ast: &Ast,
    names: &Names,
    known: &ConstValues,
    expr: Id<Expr>,
) -> Option<i128> {
    let number = |expr| const_number_with(ast, names, known, expr);
    Some(match ast.exprs[expr] {
        Expr::Int(n) => n,
        Expr::Neg(inner) => number(inner)?.checked_neg()?,
        Expr::Cast { value, .. } => number(value)?,
        Expr::Var(ref path) if let Some((_, n)) = path.int_limit() => n,
        Expr::Var(_) => {
            let def = names.expr(expr)?;
            let DefKind::Const(c) = names.def(def).kind else {
                return None;
            };
            match known.get(c) {
                Some(ConstValue::Int(n)) => n,
                Some(_) => return None,
                None => number(ast.consts[c].value)?,
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            let (a, b) = (number(lhs)?, number(rhs)?);
            match op {
                BinOp::Add => a.checked_add(b)?,
                BinOp::Subtract => a.checked_sub(b)?,
                BinOp::Multiply => a.checked_mul(b)?,
                BinOp::Divide => a.checked_div(b)?,
                BinOp::Modulo => a.checked_rem(b)?,
                BinOp::BitAnd => a & b,
                BinOp::BitOr => a | b,
                BinOp::BitXor => a ^ b,
                BinOp::ShiftLeft => a.checked_shl(u32::try_from(b).ok()?)?,
                BinOp::ShiftRight => a.checked_shr(u32::try_from(b).ok()?)?,
                _ => return None,
            }
        }
        _ => return None,
    })
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
    /// Values that become a struct → `From`'s `from` and its
    /// type arguments.
    conversions: HashMap<Id<Expr>, (DefId, Vec<TypeId>)>,
    /// Every `impl` block, indexed like `Names::impls`.
    pub impls: Vec<traits::ImplInfo>,
    /// `==` and `!=` that compare with `Eq`'s `eq`.
    trait_ops: HashMap<Id<Expr>, som_lang::LangTrait>,
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
    /// `&mut T`s used where a `&T` is expected: each lent as `&*r`.
    downgraded: HashSet<Id<Expr>>,
    /// Reads of an optional place inside `if x { … }`: they read the value it holds.
    narrowed_reads: HashSet<Id<Expr>>,
    /// Operands read through the reference they are: `self * self` of a `&i32`.
    read_through: HashSet<Id<Expr>>,
    /// `#T`s read through to what they share: `node` in `node.value`, a `&T` from its `get`.
    shared: HashSet<Id<Expr>>,
    /// References to arrays used where a slice is expected (`&numbers` for a `[i32]`), and
    /// `[mut T]`s used where a `[T]` is.
    sliced: HashMap<Id<Expr>, TypeId>,
    /// `for` loops → what they loop over.
    loops: HashMap<Id<Stmt>, Loop>,
    /// `@if`s → whether the condition holds: which branch there is.
    settled_ifs: HashMap<Id<Stmt>, bool>,
    /// The values of the constants worked out by running them, that it was checked with.
    const_values: ConstValues,
    /// `@if`s whose condition is a function's generic parameters: checked for each instance.
    regions: HashSet<Id<Stmt>>,
    /// The functions with such an `@if`.
    per_instance: HashSet<Id<Func>>,
    /// `@for f in T` over a generic type, in one instance: the copy for `T`'s struct.
    chosen_arms: HashMap<Id<Stmt>, Id<Expr>>,
    /// `match`es over a generic enum's variants: checked for each instance.
    match_regions: HashSet<Id<Expr>>,
    /// Those, in one instance: the copy for `T`'s enum.
    chosen_matches: HashMap<Id<Expr>, Id<Expr>>,
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
    /// A `&mut` lent as a `&`: `self.get()` for `fn get(&self)`, inside a method with
    /// `&mut self`, is `(&*self).get()`.
    Share,
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
    /// Each element of an array, or of a slice: how it's taken.
    Elements { take: Take },
    /// Each value an `Iterator` gives, until `next` gives `none`; `start` says where the
    /// iterator comes from. With `copied`, it gives references to values that copy, and the
    /// loop takes copies.
    Iterator { start: Start, copied: bool },
}

/// How a `for` loop takes each element of an array or slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Take {
    /// A copy: of an array it owns whose elements copy, or one it borrows.
    Copy,
    /// A reference, from `&array` or a slice, to elements that move rather than copy.
    Ref,
    /// A reference to change it, from `&mut array` or a `&mut [T]`.
    RefMut,
}

/// Where a `for` loop's iterator comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// What's looped over is the iterator: `for x in list.iter()`.
    Itself,
    /// `for x in &value`: `Iterable::iter` makes it.
    Iter,
    /// `for x in &mut value`: `IterableMut::iter_mut` makes it.
    IterMut,
    /// `for x in value`: `IntoIterator::into_iter` makes it, from the value given up.
    IntoIter,
}

som_lang::named_enum! {
    /// A method every array and slice has.
    pub enum ArrayMethod {
        /// `a.len()`: how many elements it has.
        Len = "len",
        /// `a.get(i)`: a copy of the element at `i`, or `none` past the end.
        Get = "get",
        /// `a.get_ref(i)`: a reference to the element at `i`, or `none`.
        GetRef = "get_ref",
        /// `a.get_mut(i)`: a mutable reference to the element at `i`, or `none`.
        GetMut = "get_mut",
        /// `a.set(i, v)`: stores `v` at `i`, returning whether `i` was in bounds.
        Set = "set",
        /// `s.bytes()`: a `str`'s bytes, as a `[u8]`. Only text has it.
        Bytes = "bytes",
        /// `a.slice(r)`: a view of the elements in the range `r`, or `none` if it isn't within
        /// them. Text gives text.
        Slice = "slice",
        /// `a.slice_mut(r)`: the same, as a `[mut T]`.
        SliceMut = "slice_mut",
    }
}

som_lang::named_enum! {
    /// A method every typed pointer `*T` has: raw memory operations that nothing checks, for
    /// building types like `List` in som.
    pub enum PointerMethod {
        /// `p.resize(count)`: `p` becomes room for `count` `T`s, holding what it held (as many
        /// as fit). A null `p` starts empty. Copies of the old pointer aren't valid any more.
        Resize = "resize",
        /// `p.free()`: gives the memory back.
        Free = "free",
        /// `p.read(i)`: the `T` at `i`, copied or moved out.
        Read = "read",
        /// `p.write(i, value)`: stores `value` at `i`, without reading or freeing what was there.
        Write = "write",
        /// `p.at(i)`: a reference to the `T` at `i`.
        At = "at",
        /// `p.at_mut(i)`: a `&mut` reference to the `T` at `i`.
        AtMut = "at_mut",
        /// `p.view(len)`: the `len` `T`s starting at `p`, as a `[T]`.
        View = "view",
        /// `p.view_mut(len)`: as a `[mut T]`.
        ViewMut = "view_mut",
        /// `p.is_null()`
        IsNull = "is_null",
    }
}

impl PointerMethod {
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

    /// Whether `expr` was checked and has a type: not one in an `@if` checked for each
    /// instance, or one that failed to check.
    pub fn is_checked_expr(&self, expr: Id<Expr>) -> bool {
        !matches!(self.ty(self.expr(expr)), Ty::Error)
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

    /// The trait operator expression `expr` goes through: `Eq` for `==` on a struct
    /// implementing it, `Add` for `+`.
    pub fn operator_trait(&self, expr: Id<Expr>) -> Option<som_lang::LangTrait> {
        self.trait_ops.get(&expr).copied()
    }

    /// `From`'s `from`, which `expr` is passed to to become the struct expected where it's
    /// used, with its type arguments: the struct, then `expr`'s type.
    pub fn conversion(&self, expr: Id<Expr>) -> Option<(DefId, &[TypeId])> {
        self.conversions
            .get(&expr)
            .map(|(d, args)| (*d, args.as_slice()))
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
    /// Whether `@if` `stmt`'s condition holds: `true` for its first branch, `false` for its
    /// `else`. `None` for an `if` decided when the program runs.
    /// Whether `@if` `stmt` is checked for each instance of its function, not here.
    pub fn is_region(&self, stmt: Id<Stmt>) -> bool {
        self.regions.contains(&stmt)
    }

    /// Where what's checked for each instance is: the `@if`s on a function's generic
    /// parameters, and the members only some instances have, whole. Errors found checking an
    /// instance are said if they're in one of these.
    pub fn region_spans(&self, ast: &Ast) -> Vec<som_base::Span> {
        let regions = self
            .regions
            .iter()
            .map(|&stmt| ast.stmts.span(stmt).clone());
        let members = (self.per_instance.iter())
            .filter(|&&func| !ast.funcs[func].when.is_empty())
            .map(|&func| ast.funcs.span(func).clone());
        // A function's end, where one not returning on every path is said: whether it does
        // depends on which way its `@if`s go.
        let ends = (self.per_instance.iter())
            .filter_map(|&func| ast.funcs[func].body)
            .map(|body| ast.exprs.span(body).last_char());
        let matches = self
            .match_regions
            .iter()
            .map(|&m| ast.exprs.span(m).clone());
        regions.chain(members).chain(ends).chain(matches).collect()
    }

    /// `match` `expr` with `@for (i, k) in T` among its arms, over a generic enum, in one
    /// instance: the copy written out for `T`'s enum.
    pub fn chosen_match(&self, expr: Id<Expr>) -> Option<Id<Expr>> {
        self.chosen_matches.get(&expr).copied()
    }

    /// The `@if`s checked for each instance.
    pub fn regions(&self) -> impl Iterator<Item = Id<Stmt>> + '_ {
        self.regions.iter().copied()
    }

    /// `@for f in T` over a generic type, statement `stmt`, in one instance: the copy written
    /// out for `T`'s struct, a block.
    pub fn chosen_arm(&self, stmt: Id<Stmt>) -> Option<Id<Expr>> {
        self.chosen_arms.get(&stmt).copied()
    }

    /// Whether generic function `func` is checked for each instance (it has a region).
    pub fn is_per_instance(&self, func: Id<Func>) -> bool {
        self.per_instance.contains(&func)
    }

    /// The values of the constants worked out by running them, that it was checked with.
    pub fn const_values(&self) -> &ConstValues {
        &self.const_values
    }

    /// The value constant `c` was worked out to by running it, if it was.
    pub fn const_value(&self, c: Id<som_ast::Const>) -> Option<ConstValue> {
        self.const_values.get(c)
    }

    /// `const_number`, with the constants worked out by running them.
    pub fn const_number(&self, ast: &Ast, names: &Names, expr: Id<Expr>) -> Option<i128> {
        const_number_with(ast, names, &self.const_values, expr)
    }

    pub fn settled_if(&self, stmt: Id<Stmt>) -> Option<bool> {
        self.settled_ifs.get(&stmt).copied()
    }

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

    /// Whether operand `expr`, a reference to a number or `bool`, is read through: `self *
    /// self` of a `&i32` multiplies the numbers.
    pub fn is_read_through(&self, expr: Id<Expr>) -> bool {
        self.read_through.contains(&expr)
    }

    /// Whether `expr`, a `#T`, is read through to what it shares, `node` in `node.value`:
    /// what it's used as is the `&T` its `get` gives.
    pub fn is_shared_read(&self, expr: Id<Expr>) -> bool {
        self.shared.contains(&expr)
    }

    /// Whether `expr`, a `&mut T`, is used where a `&T` is expected: it's lent as `&*expr`,
    /// so it's still there to use after.
    pub fn is_downgraded(&self, expr: Id<Expr>) -> bool {
        self.downgraded.contains(&expr)
    }

    /// Whether `expr`'s value is wrapped in an optional where it's used.
    pub fn is_wrapped(&self, expr: Id<Expr>) -> bool {
        self.wrapped.contains(&expr)
    }

    pub fn display(&self, id: TypeId, names: &Names) -> String {
        self.table.display(id, names)
    }
}
