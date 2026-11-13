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
pub use ty::{FloatTy, IntTy, Ty, TypeId, TypeTable};

use som_ast::{Ast, Expr, Func, Id, Struct};
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

    /// Whether `expr`'s value is wrapped in an optional where it's used.
    pub fn is_wrapped(&self, expr: Id<Expr>) -> bool {
        self.wrapped.contains(&expr)
    }

    pub fn display(&self, id: TypeId, names: &Names) -> String {
        self.table.display(id, names)
    }
}
