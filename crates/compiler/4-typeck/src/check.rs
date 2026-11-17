use som_ast::{Ast, BinOp, Expr, Func, Id, Settled, Span, Stmt};
use som_lang::LangTrait;
use som_resolve::{DefId, Names};
use std::collections::{HashMap, HashSet};

use crate::error::TypeError;
use crate::ty::{Ty, TypeId};
use crate::unify::Unifier;
use crate::{
    ArrayMethod, CommonField, Loop, PatternInfo, PointerMethod, Receiver, Signature, Typed,
};

mod call;
mod coerce;
mod compile_time;
mod construct;
mod diagnose;
mod expr;
mod field;
mod items;
mod lower_types;
mod matching;
mod method;
mod moves;
mod mutability;
mod narrow;
mod ops;
mod privacy;
mod stmt;
mod traits;

use call::PendingCall;
use narrow::Narrowing;

impl Settled for Checker<'_> {
    fn settled_if(&self, stmt: Id<Stmt>) -> Option<bool> {
        self.settled_ifs.get(&stmt).copied()
    }

    fn chosen_match(&self, expr: Id<Expr>) -> Option<Id<Expr>> {
        self.chosen_matches.get(&expr).copied()
    }
}

pub(crate) struct Checker<'a> {
    ast: &'a Ast,
    names: &'a Names,
    u: Unifier,
    errors: Vec<TypeError>,
    exprs: Vec<Option<TypeId>>,
    locals: HashMap<DefId, TypeId>,
    sigs: Vec<Signature>,
    structs: Vec<Vec<TypeId>>,
    /// `[value; N]` → `N`.
    counts: HashMap<Id<Expr>, TypeId>,
    /// Each value parameter's type: `usize` for `N` in `struct Buffer<N: usize>`.
    value_params: HashMap<DefId, TypeId>,
    methods: HashMap<Id<Expr>, DefId>,
    fields: HashMap<Id<Expr>, usize>,
    type_args: HashMap<Id<Expr>, Vec<TypeId>>,
    wrapped: HashSet<Id<Expr>>,
    /// `&mut T`s used where a `&T` is expected.
    downgraded: HashSet<Id<Expr>>,
    /// The optional places known to hold a value here, innermost last.
    narrowed: Vec<Narrowing>,
    next_narrowing: usize,
    /// Reads of narrowed places: they read the value, not the optional.
    narrowed_reads: HashSet<Id<Expr>>,
    /// Operands read through the reference they are: `self * self` of a `&i32`.
    read_through: HashSet<Id<Expr>>,
    shared: HashSet<Id<Expr>>,
    array_methods: HashMap<Id<Expr>, ArrayMethod>,
    pointer_methods: HashMap<Id<Expr>, PointerMethod>,
    sliced: HashMap<Id<Expr>, TypeId>,
    for_loops: HashMap<Id<Stmt>, Loop>,
    /// `@if`s → whether the condition holds.
    settled_ifs: HashMap<Id<Stmt>, bool>,
    /// The values of constants worked out by running them, so far.
    pub(crate) known: crate::ConstValues,
    /// Constants worked out by running code whose values are needed, and aren't known yet.
    needed: Vec<Id<som_ast::Const>>,
    /// `@if`s whose condition is a function's generic parameters: checked for each instance,
    /// not here.
    regions: HashSet<Id<Stmt>>,
    /// The functions with such a region.
    per_instance: HashSet<Id<Func>>,
    /// Checking one instance: what its generic parameters stand for (`N` is `3`, `T` is
    /// `f32`).
    bound: HashMap<DefId, TypeId>,
    /// `@for f in T` over a generic type, checking one instance: the copy for `T`'s struct.
    chosen_arms: HashMap<Id<Stmt>, Id<Expr>>,
    /// `match`es with `@for (i, k) in T` among their arms, over a generic enum: checked for
    /// each instance, not here.
    match_regions: HashSet<Id<Expr>>,
    /// Those, in one instance: the copy for `T`'s enum.
    chosen_matches: HashMap<Id<Expr>, Id<Expr>>,
    /// The function being checked: its return type, and where that's written.
    returns: Option<(TypeId, Option<Span>)>,
    /// The next expression inferred is the object of a field access or dereference.
    in_place: bool,
    /// Operators whose operand type wasn't known yet when they were checked.
    deferred_ops: Vec<(Id<Expr>, BinOp, TypeId, TypeId)>,
    /// Values written into text, `n` in `"total: {n}"`: whether they implement `Display`,
    /// once what they are is known.
    displayed: Vec<(Id<Expr>, TypeId)>,
    /// Calls ending a block, its final value, without a `;`: one giving nothing needs it,
    /// once what it gives is known.
    tail_calls: Vec<(Id<Expr>, TypeId)>,
    /// Inferring an assignment's target: `s[i] = v`, which a slice can't do.
    assigning: bool,
    /// Values to convert through `From` whose types weren't known well enough to choose
    /// which, until later: `println(xs.get(0))` before what `xs` holds is. With where what's
    /// expected came from.
    deferred_conversions: Vec<(Id<Expr>, TypeId, TypeId, Option<Span>)>,
    /// How many `loop`s and `while`s enclose the statement being checked.
    loops: usize,
    patterns: HashMap<Id<som_ast::Pattern>, PatternInfo>,
    receivers: HashMap<Id<Expr>, Receiver>,
    common: Vec<Vec<CommonField>>,
    diverging: HashSet<Id<Expr>>,
    /// The next expression checked is a statement's: its value isn't used.
    discarding: bool,
    /// The type the next expression inferred is expected to have, if `expect` knows it.
    expected: Option<TypeId>,
    /// The type the expression being inferred is expected to have: for a call of a function
    /// that shares its name with others, it helps decide which.
    expected_here: Option<TypeId>,
    /// Calls and function values → which of the functions sharing their name they name.
    callees: HashMap<Id<Expr>, DefId>,
    /// Lowering a `let` annotation, where a generic struct's type arguments can be left out:
    /// `let list: List = [1, 2]`.
    omitted_args_ok: bool,
    /// The type aliases being expanded, innermost last: one met again refers to itself.
    expanding: Vec<DefId>,
    /// Aliases already reported as referring to themselves, so each is reported once.
    cyclic: HashSet<DefId>,
    /// What the generic parameters of the aliases being expanded stand for. Outside an
    /// expansion, a parameter stands for itself.
    alias_args: HashMap<DefId, TypeId>,
    /// Values that become a struct through its `From` implementation: the trait's `from`
    /// and its type arguments.
    conversions: HashMap<Id<Expr>, (DefId, Vec<TypeId>)>,
    /// Every `impl` block, indexed like `Names::impls`.
    impls: Vec<crate::traits::ImplInfo>,
    /// The function whose signature or body is being checked.
    current_func: Option<Id<Func>>,
    /// Types that have to implement a trait, to check once they're known, and where.
    obligations: Vec<(TypeId, DefId, Span)>,
    /// `==` and `!=` comparing with `Eq`'s `eq`.
    trait_ops: HashMap<Id<Expr>, LangTrait>,
    /// The types of the `@unreachable`s: what they're used as, or `()`.
    unreachables: Vec<TypeId>,
    /// Calls that more than one function fits, decided once the rest of the program says more
    /// about what their result has to be.
    pending_calls: Vec<PendingCall>,
    /// Functions whose bodies have been checked, or are being.
    checked: HashSet<Id<Func>>,
    /// Constants whose values have been checked, or are being.
    checked_consts: HashSet<Id<som_ast::Const>>,
}

impl<'a> Checker<'a> {
    pub fn new(ast: &'a Ast, names: &'a Names) -> Self {
        Self::with_table(ast, names, crate::TypeTable::default())
    }

    /// One whose types are added to `table`, so the ids of the types it has stay theirs.
    pub fn with_table(ast: &'a Ast, names: &'a Names, table: crate::TypeTable) -> Self {
        let mut checker = Checker {
            ast,
            names,
            u: Unifier::with_table(table),
            errors: Vec::new(),
            exprs: vec![None; ast.exprs.len()],
            locals: HashMap::new(),
            sigs: Vec::new(),
            structs: Vec::new(),
            counts: HashMap::new(),
            value_params: HashMap::new(),
            methods: HashMap::new(),
            fields: HashMap::new(),
            type_args: HashMap::new(),
            wrapped: HashSet::new(),
            downgraded: HashSet::new(),
            narrowed: Vec::new(),
            next_narrowing: 0,
            narrowed_reads: HashSet::new(),
            read_through: HashSet::new(),
            shared: HashSet::new(),
            array_methods: HashMap::new(),
            pointer_methods: HashMap::new(),
            sliced: HashMap::new(),
            for_loops: HashMap::new(),
            settled_ifs: HashMap::new(),
            known: crate::ConstValues::default(),
            needed: Vec::new(),
            regions: HashSet::new(),
            per_instance: HashSet::new(),
            bound: HashMap::new(),
            chosen_arms: HashMap::new(),
            match_regions: HashSet::new(),
            chosen_matches: HashMap::new(),
            returns: None,
            in_place: false,
            deferred_ops: Vec::new(),
            displayed: Vec::new(),
            tail_calls: Vec::new(),
            assigning: false,
            deferred_conversions: Vec::new(),
            loops: 0,
            patterns: HashMap::new(),
            receivers: HashMap::new(),
            common: Vec::new(),
            diverging: HashSet::new(),
            discarding: false,
            expected: None,
            expected_here: None,
            callees: HashMap::new(),
            pending_calls: Vec::new(),
            checked: HashSet::new(),
            checked_consts: HashSet::new(),
            omitted_args_ok: false,
            expanding: Vec::new(),
            cyclic: HashSet::new(),
            alias_args: HashMap::new(),
            impls: Vec::new(),
            current_func: None,
            obligations: Vec::new(),
            trait_ops: HashMap::new(),
            unreachables: Vec::new(),
            conversions: HashMap::new(),
        };
        // Text is the standard library's `string`, where it's loaded.
        if let Some(def) = names.lang_struct(som_lang::LangStruct::String) {
            checker.u.table.text = checker.u.table.intern(Ty::Struct(def, Vec::new()));
        }
        checker
    }

    /// Checks the program: what it found, its errors, and the constants worked out by running
    /// code it needed and didn't know (`needed`).
    pub fn run_needing(mut self) -> (Typed, Vec<TypeError>, Vec<Id<som_ast::Const>>) {
        self.check_program();
        let needed = std::mem::take(&mut self.needed);
        let (typed, errors) = self.finish();
        (typed, errors, needed)
    }

    /// Checks one instance of generic function `func`, its generic parameters standing for
    /// `args` (as `som_typeck::check_instance` says).
    pub fn run_instance(mut self, func: Id<Func>, args: &[TypeId]) -> (Typed, Vec<TypeError>) {
        let params = self.names.type_params(func).to_vec();
        for (def, &arg) in params.into_iter().zip(args) {
            self.bound.insert(def, arg);
        }
        self.check_items(Some(func));
        self.finish()
    }

    fn span(&self, expr: Id<Expr>) -> Span {
        self.ast.exprs.span(expr).clone()
    }
}
