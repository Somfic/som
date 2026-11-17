use som_ast::{Ast, BinOp, Expr, Func, Id, Settled, Span, Stmt};
use som_resolve::{DefId, Names};
use std::collections::{HashMap, HashSet};

use crate::error::TypeError;
use crate::ty::{Ty, TypeId};
use crate::unify::Unifier;
use crate::{Records, Signature, Typed};

mod call;
mod coerce;
mod compile_time;
mod construct;
mod conversions;
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
mod operator_traits;
mod ops;
mod privacy;
mod stmt;
mod traits;

use call::PendingCall;
use narrow::Narrowing;

impl Settled for Checker<'_> {
    fn settled_if(&self, stmt: Id<Stmt>) -> Option<bool> {
        self.records.settled_ifs.get(&stmt).copied()
    }

    fn chosen_match(&self, expr: Id<Expr>) -> Option<Id<Expr>> {
        self.records.chosen_matches.get(&expr).copied()
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
    type_args: HashMap<Id<Expr>, Vec<TypeId>>,
    sliced: HashMap<Id<Expr>, TypeId>,
    /// Values that become a struct through its `From`: the trait's `from` and its type arguments.
    conversions: HashMap<Id<Expr>, (DefId, Vec<TypeId>)>,
    records: Records,
    /// Every `impl` block, indexed like `Names::impls`.
    impls: Vec<crate::impls::ImplInfo>,
    hints: Hints,
    deferred: Deferred,
    value_params: HashMap<DefId, TypeId>,
    /// Optional places known to hold a value here, innermost last.
    narrowed: Vec<Narrowing>,
    next_narrowing: usize,
    pub(crate) known: crate::ConstValues,
    /// Constants needed before their values are known: worked out by running code.
    needed: Vec<Id<som_ast::Const>>,
    /// Checking one instance: what its generic parameters stand for.
    bound: HashMap<DefId, TypeId>,
    returns: Option<(TypeId, Option<Span>)>,
    current_func: Option<Id<Func>>,
    loop_depth: usize,
    /// Aliases being expanded, innermost last: one met again refers to itself.
    expanding: Vec<DefId>,
    /// Aliases already reported as referring to themselves, so each is reported once.
    cyclic: HashSet<DefId>,
    /// What the generic parameters of aliases being expanded stand for; outside an expansion,
    /// a parameter stands for itself.
    alias_args: HashMap<DefId, TypeId>,
    /// Functions whose bodies have been checked, or are being.
    checked: HashSet<Id<Func>>,
    checked_consts: HashSet<Id<som_ast::Const>>,
}

/// What the expression about to be inferred is to its caller: set just before `infer`, which
/// takes them.
#[derive(Default)]
struct Hints {
    /// It's the object of a field access or dereference.
    in_place: bool,
    /// It's a statement's: its value isn't used.
    discarding: bool,
    /// It's an assignment's target: `s[i] = v`, which a slice can't do.
    assigning: bool,
    expected: Option<TypeId>,
    /// What the expression being inferred is expected to be: picks among same-named functions.
    expected_here: Option<TypeId>,
    /// Lowering a `let` annotation, where a generic struct's type arguments can be left out:
    /// `let list: List = [1, 2]`.
    omitted_args_ok: bool,
}

/// Checks that wait until the rest of the program says more about the types involved.
#[derive(Default)]
struct Deferred {
    /// Operators whose operand type wasn't known yet.
    ops: Vec<(Id<Expr>, BinOp, TypeId, TypeId)>,
    /// Values written into text, `n` in `"total: {n}"`: whether they implement `Display`.
    displayed: Vec<(Id<Expr>, TypeId)>,
    /// Calls ending a block without a `;`: one giving nothing needs it.
    tail_calls: Vec<(Id<Expr>, TypeId)>,
    /// `From` conversions whose types weren't known well enough to choose one, with where what's
    /// expected came from.
    conversions: Vec<(Id<Expr>, TypeId, TypeId, Option<Span>)>,
    obligations: Vec<(TypeId, DefId, Span)>,
    /// The types of the `@unreachable`s: what they're used as, or `()`.
    unreachables: Vec<TypeId>,
    /// Calls that more than one function fits.
    calls: Vec<PendingCall>,
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
            type_args: HashMap::new(),
            sliced: HashMap::new(),
            conversions: HashMap::new(),
            records: Records::default(),
            impls: Vec::new(),
            hints: Hints::default(),
            deferred: Deferred::default(),
            value_params: HashMap::new(),
            narrowed: Vec::new(),
            next_narrowing: 0,
            known: crate::ConstValues::default(),
            needed: Vec::new(),
            bound: HashMap::new(),
            returns: None,
            current_func: None,
            loop_depth: 0,
            expanding: Vec::new(),
            cyclic: HashSet::new(),
            alias_args: HashMap::new(),
            checked: HashSet::new(),
            checked_consts: HashSet::new(),
        };
        if let Some(def) = names.lang_struct(som_lang::LangStruct::String) {
            checker.u.table.text = checker.u.table.intern(Ty::Struct(def, Vec::new()));
        }
        checker
    }

    /// Also gives the constants it needed and didn't know, to work out by running code.
    pub fn run_needing(mut self) -> (Typed, Vec<TypeError>, Vec<Id<som_ast::Const>>) {
        self.check_program();
        let needed = std::mem::take(&mut self.needed);
        let (typed, errors) = self.finish();
        (typed, errors, needed)
    }

    /// Checks generic function `func` with its parameters standing for `args`.
    pub fn run_instance(mut self, func: Id<Func>, args: &[TypeId]) -> (Typed, Vec<TypeError>) {
        let params = self.names.type_params(func).to_vec();
        for (def, &arg) in params.into_iter().zip(args) {
            self.bound.insert(def, arg);
        }
        self.check_items(Some(func));
        self.finish()
    }

    /// `member_holds`, in a program already checked, whose impl blocks are `impls`.
    pub(crate) fn member_holds_in(
        mut self,
        impls: Vec<crate::impls::ImplInfo>,
        func: Id<Func>,
        receiver: TypeId,
    ) -> Option<bool> {
        self.impls = impls;
        self.member_holds(func, receiver)
    }

    /// Takes the hints, for checking a body elsewhere from inside an expression; `assigning`
    /// stays, since the target is still being inferred.
    fn take_hints(&mut self) -> Hints {
        let assigning = self.hints.assigning;
        std::mem::replace(
            &mut self.hints,
            Hints {
                assigning,
                ..Hints::default()
            },
        )
    }

    fn span(&self, expr: Id<Expr>) -> Span {
        self.ast.exprs.span(expr).clone()
    }
}
