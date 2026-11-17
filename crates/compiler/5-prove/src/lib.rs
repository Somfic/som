//! Checks what has to be known where it's needed, once every type is: that an index is
//! inside what it indexes, and that each `@where` of a function holds where it's called.
//! Nothing is checked as the program runs, so what can't be known here isn't allowed.
//!
//! Each function body is gone through in the order it runs, learning what holds as it
//! goes (see [`facts`]): an `if`'s condition inside it, and its opposite after it when it
//! returns; a `while`'s condition; a `for` loop's range; a `let`'s value; the function's
//! own `@where`s. What's learned about a place is forgotten when it may change: assigned,
//! lent as `&mut`, or given to a method taking `&mut self`.

mod calls;
mod changes;
pub mod error;
pub mod facts;
mod index;
mod values;
mod walk;

use som_ast::{Ast, Expr, Func, Id, Stmt};
use som_resolve::{DefId, DefKind, Names};
use som_typeck::Typed;
use som_typeck::{Ty, TypeId};

pub use error::ProveError;
use facts::{Facts, Term, Value};
use som_base::Span;
use std::collections::HashMap;

pub fn prove(ast: &Ast, names: &Names, typed: &Typed) -> Vec<ProveError> {
    let mut prover = Prover::new(ast, names, typed, HashMap::new());
    for (id, _) in ast.funcs.iter() {
        prover.check_wheres(id);
    }
    for (id, func) in ast.funcs.iter() {
        // A member only some instances have is proven for each: what's in it depends on them.
        if typed.is_per_instance(id) && !func.when.is_empty() {
            continue;
        }
        if let Some(body) = func.body {
            prover.facts = Facts::default();
            prover.assume_requires(id);
            prover.expr(body);
        }
    }
    // A constant's value, worked out while compiling: its calls' `@where`s too, `at(5)` of
    // `@where i < 3` an error here rather than whatever running it gives.
    for (_, constant) in ast.consts.iter() {
        if typed.is_checked_expr(constant.value) {
            prover.facts = Facts::default();
            prover.expr(constant.value);
        }
    }
    prover.errors
}

/// Proves one instance of function `func`, checked for that instance (`typed`), its generic
/// value parameters standing for `values` (`N` for 3): its body, where `@if N >= 3` is
/// known. Only what's wrong inside `regions` (its `@if`s on them) is said: the rest of it was
/// proven once, for every instance.
pub fn prove_instance(
    ast: &Ast,
    names: &Names,
    typed: &Typed,
    func: Id<som_ast::Func>,
    values: &[(DefId, i128)],
    regions: &[Span],
) -> Vec<ProveError> {
    let mut prover = Prover::new(ast, names, typed, values.iter().copied().collect());
    if let Some(body) = ast.funcs[func].body {
        prover.assume_requires(func);
        prover.expr(body);
    }
    (prover.errors.into_iter())
        .filter(|error| {
            regions
                .iter()
                .any(|region| starts_inside(error.span(), region))
        })
        .collect()
}

fn starts_inside(span: &Span, region: &Span) -> bool {
    region.source.identifier() == span.source.identifier()
        && region.start_offset <= span.start_offset
        && span.start_offset < region.start_offset + region.length
}

/// What facts are about: a variable or a field or property of one, `xs.len`; or an
/// array's length that isn't written out, `N` of `[T; N]`.
#[derive(Clone, PartialEq, Debug)]
pub enum Place {
    Path { root: DefId, path: Vec<Box<str>> },
    Len(DefId),
}

impl Place {
    fn var(root: DefId) -> Place {
        Place::Path {
            root,
            path: Vec::new(),
        }
    }

    /// Whether this is `other`, or inside it: `xs.len` is inside `xs`.
    fn within(&self, other: &Place) -> bool {
        match (self, other) {
            (
                Place::Path { root, path },
                Place::Path {
                    root: other_root,
                    path: other_path,
                },
            ) => root == other_root && path.starts_with(other_path),
            _ => false,
        }
    }
}

impl Value<Place> {
    fn term_place(&self) -> &Place {
        match &self.term {
            Term::Place(place) => place,
            Term::Zero => unreachable!("made from a place"),
        }
    }

    /// The variable or field this is, as it is: not `xs.len - 1`.
    fn into_path(self) -> Option<(DefId, Vec<Box<str>>)> {
        match self {
            Value {
                term: Term::Place(Place::Path { root, path }),
                minus: Term::Zero,
                offset: 0,
            } => Some((root, path)),
            _ => None,
        }
    }
}

/// How a condition's parameters are read: at a call, as the arguments given for them.
type Subst<'s> = &'s dyn Fn(DefId) -> Option<Value<Place>>;

fn as_is(_: DefId) -> Option<Value<Place>> {
    None
}

struct Prover<'a> {
    ast: &'a Ast,
    names: &'a Names,
    typed: &'a Typed,
    facts: Facts<Place>,
    errors: Vec<ProveError>,
    /// The statements being gone through, innermost last: where a fix puts a check.
    within: Vec<Within>,
    /// Proving one instance: the numbers its generic value parameters stand for.
    bound: HashMap<DefId, i128>,
}

/// A statement being gone through, with the rest of its block after it, and whether the
/// block ends in a value.
struct Within {
    stmt: Id<Stmt>,
    rest: Vec<Id<Stmt>>,
    tail: bool,
}

impl<'a> Prover<'a> {
    fn new(ast: &'a Ast, names: &'a Names, typed: &'a Typed, bound: HashMap<DefId, i128>) -> Self {
        Prover {
            ast,
            names,
            typed,
            facts: Facts::default(),
            errors: Vec::new(),
            within: Vec::new(),
            bound,
        }
    }
}

impl Prover<'_> {
    fn callee(&self, call: Id<Expr>) -> Option<Id<Func>> {
        match self.names.def(self.typed.callee(call, self.names)?).kind {
            DefKind::Func(func) => Some(func),
            _ => None,
        }
    }

    fn method(&self, expr: Id<Expr>) -> Option<Id<Func>> {
        match self.names.def(self.typed.method(expr)?).kind {
            DefKind::Func(func) => Some(func),
            _ => None,
        }
    }

    /// Whether method `func` takes its `self` as `&mut`, and so may change it.
    fn takes_mut_self(&self, func: Id<Func>) -> bool {
        (self.typed.func(func).params.first())
            .is_some_and(|&p| matches!(self.typed.ty(p), Ty::Ref { mutable: true, .. }))
    }

    fn local_var(&self, expr: Id<Expr>) -> Option<DefId> {
        let Expr::Var(_) = self.ast.exprs[expr] else {
            return None;
        };
        let def = self.names.expr(expr)?;
        matches!(
            self.names.def(def).kind,
            DefKind::Local(_) | DefKind::Param { .. } | DefKind::Binding(_)
        )
        .then_some(def)
    }

    /// Whether `expr` reads variable `def`.
    fn mentions(&self, expr: Id<Expr>, def: DefId) -> bool {
        if let Expr::Var(_) = self.ast.exprs[expr]
            && self.names.expr(expr) == Some(def)
        {
            return true;
        }
        let mut found = false;
        self.each_child(expr, &mut |child| found |= self.mentions(child, def));
        found
    }

    fn expr_ty(&self, expr: Id<Expr>) -> Option<TypeId> {
        Some(self.typed.expr(expr))
    }

    fn is_bool(&self, expr: Id<Expr>) -> bool {
        self.expr_ty(expr)
            .is_some_and(|ty| matches!(self.typed.ty(ty), Ty::Bool))
    }

    fn is_integer(&self, expr: Id<Expr>) -> bool {
        self.expr_ty(expr)
            .is_some_and(|ty| matches!(self.typed.ty(self.through_ref(ty)), Ty::Int(_)))
    }

    fn is_unsigned(&self, expr: Id<Expr>) -> bool {
        self.expr_ty(expr).is_some_and(|ty| self.is_unsigned_ty(ty))
    }

    fn is_unsigned_ty(&self, ty: TypeId) -> bool {
        matches!(self.typed.ty(self.through_ref(ty)), Ty::Int(int) if !int.is_signed())
    }

    fn through_ref(&self, ty: TypeId) -> TypeId {
        match *self.typed.ty(ty) {
            Ty::Ref { to, .. } => to,
            _ => ty,
        }
    }
}
