use crate::error::Unused;
use som_ast::{Ast, BinOp, Expr, Func, Id, Pattern, Primitive, Span, Stmt, TypeExpr, VariantKind};
use som_resolve::visibility::{self, Hidden, Viewer};
use som_resolve::{DefId, DefKind, LangTrait, Names};
use std::collections::{HashMap, HashSet};

use crate::error::{NotForC, TypeError};
use crate::traits::find_impl;
use crate::ty::{IntTy, Ty, TypeId, Value};
use crate::unify::{Unifier, VarKind};
use crate::{
    ArrayMethod, CommonField, Loop, PatternInfo, PointerMethod, Receiver, Signature, Take, Typed,
};
use som_base::Message;
use som_base::message;
use som_base::{Fix, msg};

mod matching;
mod moves;
mod traits;

use matching::holds;

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
        let mut checker = Checker {
            ast,
            names,
            u: Unifier::new(),
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
        if let Some(def) = names.lang_struct(som_resolve::LangStruct::String) {
            checker.u.table.text = checker.u.table.intern(Ty::Struct(def, Vec::new()));
        }
        checker
    }

    pub fn run(mut self) -> (Typed, Vec<TypeError>) {
        let ast = self.ast;

        // The types of value parameters, which the types of fields and parameters can use.
        let impl_blocks =
            (ast.modules.iter())
                .flat_map(|m| &m.items)
                .filter_map(|item| match item {
                    som_ast::Item::Impl(block) => Some(block),
                    _ => None,
                });
        let aliases = (ast.modules.iter())
            .flat_map(|m| &m.items)
            .filter_map(|item| match item {
                som_ast::Item::Alias(alias) => Some(alias),
                _ => None,
            })
            .zip(self.names.aliases())
            .flat_map(|(alias, &def)| alias.generics.iter().zip(self.names.alias_params(def)));
        let generics = (ast.structs.iter())
            .flat_map(|(id, s)| s.generics.iter().zip(self.names.struct_params(id)))
            .chain(
                impl_blocks
                    .zip(&self.names.impls)
                    .flat_map(|(block, def)| block.generics.iter().zip(&def.params)),
            )
            .chain(
                (ast.funcs.iter())
                    .flat_map(|(id, f)| f.generics.iter().zip(self.names.type_params(id))),
            )
            .chain(aliases);
        for (generic, &def) in generics.collect::<Vec<_>>() {
            if let Some(ty) = generic.ty {
                let lowered = self.lower(ty);
                if !matches!(self.u.get(lowered), Ty::Int(_) | Ty::Bool | Ty::Error) {
                    self.errors.push(TypeError::InvalidValueParamType {
                        ty: self.describe(lowered),
                        span: self.ast.types.span(ty).clone(),
                    });
                }
                self.value_params.insert(def, lowered);
            }
        }

        // Every type alias, used or not, so one that refers to itself is reported.
        // Its parameters stand for themselves.
        for def in self.names.aliases().to_vec() {
            if let DefKind::Alias { ty } = self.names.def(def).kind {
                self.expanding.push(def);
                self.lower(ty);
                self.expanding.pop();
            }
        }

        // Struct fields and function signatures first, so bodies can use them in any order.
        for (_, s) in ast.structs.iter() {
            // An enum's payload: every variant's fields, one variant after another.
            let fields = (s.fields.iter().chain(s.payload()))
                .map(|f| self.lower(f.ty))
                .collect();
            self.structs.push(fields);
        }
        for (id, s) in ast.structs.iter() {
            let common = self.common_fields(id, s);
            self.common.push(common);
        }
        // Impl blocks' targets and associated types, which signatures can name.
        self.build_impls();
        for (id, func) in ast.funcs.iter() {
            self.current_func = Some(id);
            let params = func
                .params
                .iter()
                .map(|p| {
                    let ty = match p.ty {
                        Some(ty) => self.lower(ty),
                        None => self.u.fresh(VarKind::Any),
                    };
                    // Passed as a pointer to the caller's variable.
                    match p.out {
                        true => self.u.intern(Ty::Ref {
                            mutable: true,
                            to: ty,
                        }),
                        false => ty,
                    }
                })
                .collect();
            let ret = match (func.returns, func.body) {
                // `prop xy: Vector = …`: a property's type can leave its arguments to its value,
                // as a `let`'s can.
                (Some(ty), _) if func.prop => {
                    self.omitted_args_ok = true;
                    let ret = self.lower(ty);
                    self.omitted_args_ok = false;
                    ret
                }
                (Some(ty), _) => self.lower(ty),
                (None, None) => self.u.table.unit,
                (None, Some(_)) => self.u.fresh(VarKind::Any),
            };
            self.sigs.push(Signature { params, ret });
            for (i, &def) in self.names.params(id).iter().enumerate() {
                self.locals.insert(def, self.sigs[id.id].params[i]);
            }
        }

        self.current_func = None;
        self.check_impls();
        // `@where i < self.len`: conditions, in terms of the parameters.
        let bool = self.u.table.bool;
        for (id, func) in ast.funcs.iter() {
            self.current_func = Some(id);
            for clause in &func.requires {
                self.expect(clause.condition, bool, None);
            }
        }
        self.current_func = None;
        // Each with a name: one written in place is checked there, with what's expected of it.
        for (id, c) in ast.consts.iter() {
            if !c.in_place {
                self.check_const(id);
            }
        }
        for (id, func) in ast.funcs.iter() {
            if func.body.is_some() {
                self.check_func(id);
                self.settle(func.name.span.clone());
            }
        }
        // Once bodies are checked: what a function gives can be what its body is.
        self.check_listeners();
        self.current_func = None;

        // After the bodies, which decide unwritten return types and what results are used as.
        self.current_func = None;
        self.resolve_pending_calls();
        self.check_overloads();
        self.check_obligations();

        // Conversions chosen once what they convert is known. If it still isn't, that's
        // reported where it's made, not here.
        for (expr, expected, found, from) in std::mem::take(&mut self.deferred_conversions) {
            if self.from_conversion(expr, expected, found)
                || self.conversion_undecided(expected, found)
            {
                continue;
            }
            self.mismatch_at(expr, expected, found, from);
        }

        // `{ draw(); end() }`: a call giving nothing ends with a `;`, as a statement; only a
        // value is left without one.
        for (call, ty) in std::mem::take(&mut self.tail_calls) {
            if matches!(self.u.get(self.u.shallow(ty)), Ty::Unit) {
                self.errors.push(TypeError::UnitTail {
                    span: self.span(call),
                });
            }
        }
        // `"total: {n}"`: what's written in has to be shown as text. Through references.
        if let Some(display) = self.names.lang(LangTrait::Display) {
            for (expr, ty) in std::mem::take(&mut self.displayed) {
                // `"{5}"`: a number nothing else decides is an `i32` or `f64`.
                self.default_numbers(ty);
                let mut shown = self.u.shallow(ty);
                while let Ty::Ref { to, .. } = *self.u.get(shown) {
                    shown = self.u.shallow(to);
                }
                if !self.implements(shown, display) {
                    self.errors.push(TypeError::NotDisplay {
                        ty: self.describe(ty),
                        span: self.span(expr),
                    });
                }
            }
        }
        for (expr, op, lhs, rhs) in std::mem::take(&mut self.deferred_ops) {
            if self.trait_op(expr, op, lhs).is_some() {
                continue;
            }
            if self.operand_ok(op, lhs) == Some(false) || self.u.var_kind(lhs) == Some(VarKind::Any)
            {
                self.invalid_operands(expr, op, lhs, rhs);
            }
        }

        self.finish()
    }

    /// Checks function `id`'s body, once: in order, or sooner, when a call needs the return
    /// type it decides because it isn't written. Whatever the body being checked had going
    /// is put back afterwards.
    fn check_func(&mut self, id: Id<Func>) {
        let func = &self.ast.funcs[id];
        let Some(body) = func.body else { return };
        if !self.checked.insert(id) {
            return;
        }
        let returns = self.returns.take();
        let narrowed = std::mem::take(&mut self.narrowed);
        let outer = (
            self.current_func.replace(id),
            std::mem::take(&mut self.in_place),
            std::mem::take(&mut self.loops),
            std::mem::take(&mut self.discarding),
            self.expected.take(),
            self.expected_here.take(),
            std::mem::take(&mut self.omitted_args_ok),
        );
        self.check_body(id, func, body);
        self.check_moves_out(body);
        self.check_main_params(id, func);
        self.check_main_returns(id, func, body);
        // After the body, which decides a return type that isn't written.
        if func.prop {
            self.check_prop(id, func);
        }
        self.returns = returns;
        self.narrowed = narrowed;
        (
            self.current_func,
            self.in_place,
            self.loops,
            self.discarding,
            self.expected,
            self.expected_here,
            self.omitted_args_ok,
        ) = outer;
    }

    /// Whether `expr` is a number known while compiling: a literal, a constant made of them,
    /// or a function's value parameter, `N`, known in each instance.
    fn settled_int(&self, expr: Id<Expr>) -> bool {
        if self.const_number(expr).is_some() {
            return true;
        }
        match &self.ast.exprs[expr] {
            Expr::Var(_) => self.names.expr(expr).is_some_and(|def| {
                matches!(
                    self.names.def(def).kind,
                    DefKind::TypeParam { value: true, .. }
                        | DefKind::ImplParam { value: true, .. }
                        | DefKind::StructParam { value: true, .. }
                )
            }),
            Expr::Binary { lhs, rhs, .. } => self.settled_int(*lhs) && self.settled_int(*rhs),
            Expr::Cast { value, .. } | Expr::Neg(value) => self.settled_int(*value),
            _ => false,
        }
    }

    /// Whether what an `@for` loops over is known while compiling: a range of known numbers,
    /// or an array written out, each of its elements a copy of the body.
    fn settled_iterable(&self, expr: Id<Expr>) -> bool {
        match &self.ast.exprs[expr] {
            Expr::Range {
                start: Some(start),
                end: Some(end),
            } => self.settled_int(*start) && self.settled_int(*end),
            Expr::Array(_) => true,
            _ => false,
        }
    }

    /// The value of `@if`'s condition `expr`, if it's known while compiling: a `bool`, a
    /// constant that's one, comparisons of known numbers, and `!`, `&&` and `||` of those.
    fn settled_bool(&self, expr: Id<Expr>) -> Option<bool> {
        match &self.ast.exprs[expr] {
            Expr::Bool(b) => Some(*b),
            Expr::Not(inner) => self.settled_bool(*inner).map(|b| !b),
            Expr::Var(_) => match self.names.def(self.names.expr(expr)?).kind {
                DefKind::Const(c) => self.settled_bool(self.ast.consts[c].value),
                _ => None,
            },
            Expr::Binary { op, lhs, rhs } => match op {
                BinOp::And => Some(self.settled_bool(*lhs)? && self.settled_bool(*rhs)?),
                BinOp::Or => Some(self.settled_bool(*lhs)? || self.settled_bool(*rhs)?),
                _ => {
                    if let (Some(a), Some(b)) = (self.settled_bool(*lhs), self.settled_bool(*rhs)) {
                        return match op {
                            BinOp::Equals => Some(a == b),
                            BinOp::NotEquals => Some(a != b),
                            _ => None,
                        };
                    }
                    let (a, b) = (self.const_number(*lhs)?, self.const_number(*rhs)?);
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

    /// Each `@on fail` function is called with a `&Failure`, and gives nothing back.
    fn check_listeners(&mut self) {
        let failure = self.names.lang_struct(som_resolve::LangStruct::Failure);
        for &id in self.names.fail_listeners() {
            let sig = &self.sigs[id.id];
            let takes = matches!(&sig.params[..], [param] if matches!(
                self.u.table.get(*param),
                Ty::Ref { mutable: false, to }
                    if matches!(self.u.table.get(*to), Ty::Struct(def, _) if Some(*def) == failure)
            ));
            let ret = self.u.shallow(sig.ret);
            let gives = matches!(self.u.table.get(ret), Ty::Unit);
            let func = &self.ast.funcs[id];
            if !takes || !gives || !func.generics.is_empty() {
                self.errors.push(TypeError::ListenerSignature {
                    span: func.name.span.clone(),
                });
            }
        }
    }

    /// Checks constant `id`'s value, once: in order, or sooner, when a value naming it needs
    /// its type. Without a written type, it's what its value would be with nothing else
    /// said: `60` is an `i32`, wherever it's used.
    fn check_const(&mut self, id: Id<som_ast::Const>) -> TypeId {
        let def = self.names.const_def(id);
        if !self.checked_consts.insert(id) {
            // Made of itself, which is reported as that.
            return self.locals.get(&def).copied().unwrap_or(self.u.table.error);
        }
        let c = &self.ast.consts[id];
        let outer = (
            self.current_func.take(),
            self.returns.take(),
            std::mem::take(&mut self.narrowed),
            self.expected.take(),
            self.expected_here.take(),
        );
        let ty = match c.ty {
            Some(written) => {
                let ty = self.lower(written);
                // Known before the value is checked, for a use inside it.
                self.locals.insert(def, ty);
                let from = self.ast.types.span(written).clone();
                self.expect(c.value, ty, Some(from));
                ty
            }
            None => {
                let ty = self.infer(c.value);
                self.text_as_string(ty);
                self.default_numbers(ty);
                ty
            }
        };
        self.locals.insert(def, ty);
        self.settle(c.name.span.clone());
        (
            self.current_func,
            self.returns,
            self.narrowed,
            self.expected,
            self.expected_here,
        ) = outer;
        ty
    }

    /// What `main` gives is the program's exit code: an integer, or nothing. A script ending
    /// in `"done"` gives a `string`, which can't be one.
    fn check_main_returns(&mut self, id: Id<Func>, func: &Func, body: Id<Expr>) {
        let def = self.names.func(id);
        let top_level = self.ast.modules[self.names.def(def).module].name.is_empty()
            && self.names.impl_of(id).is_none();
        if &*func.name.name != "main" || !top_level {
            return;
        }
        // A literal it ends in: text is a `string`, a number what it would be anyway.
        let ret = self.sigs[id.id].ret;
        self.text_as_string(ret);
        if self.u.var_kind(ret) == Some(VarKind::Float) {
            self.default_numbers(ret);
        }
        let ret = self.u.shallow(ret);
        let fine = match self.u.var_kind(ret) {
            // An integer nothing has decided; or what isn't known at all, reported elsewhere.
            Some(VarKind::Int | VarKind::Any) => true,
            // A collection, `[1, 2]`.
            Some(_) => false,
            None => matches!(self.u.get(ret), Ty::Int(_) | Ty::Unit | Ty::Error),
        };
        if fine {
            return;
        }
        let value = match &self.ast.exprs[body] {
            Expr::Block {
                value: Some(value), ..
            } => *value,
            _ => body,
        };
        let span = match func.returns {
            Some(ty) => self.ast.types.span(ty).clone(),
            None => self.span(value),
        };
        self.errors.push(TypeError::MainReturns {
            ty: self.describe(ret),
            span,
            written: func.returns.is_some(),
        });
    }

    /// `fn main(args: &[cstr])`: the program's entry takes nothing; its arguments are read
    /// with `args()`, from anywhere.
    fn check_main_params(&mut self, id: Id<Func>, func: &Func) {
        let def = self.names.func(id);
        let top_level = self.ast.modules[self.names.def(def).module].name.is_empty()
            && self.names.impl_of(id).is_none();
        let (Some(first), Some(last)) = (func.params.first(), func.params.last()) else {
            return;
        };
        if &*func.name.name != "main" || !top_level {
            return;
        }
        let end = match last.ty {
            Some(ty) => self.ast.types.span(ty).clone(),
            None => last.name.span.clone(),
        };
        self.errors.push(TypeError::MainParams {
            span: first.name.span.merge(&end),
        });
    }

    fn check_body(&mut self, id: Id<Func>, func: &Func, body: Id<Expr>) {
        let ret = self.sigs[id.id].ret;
        let from = func.returns.map(|t| self.ast.types.span(t).clone());
        self.returns = Some((ret, from.clone()));
        // What it returns is what its value is expected to be: it can choose an overload.
        self.expected = Some(ret);
        let body_ty = self.infer(body);
        // A body without a final value is fine if no path gets to its end: every one returns.
        if let Expr::Block { stmts, value: None } = &self.ast.exprs[body] {
            if self.leaves(stmts) {
                return;
            }
            // Some path gets to the end with nothing to return.
            let unit = self.u.table.unit;
            if self.u.unify(ret, unit).is_err() {
                self.errors.push(TypeError::MissingReturn {
                    ty: self.describe(ret),
                    span: self.ast.exprs.span(body).last_char(),
                    returns: from,
                });
                return;
            }
        }
        if !self.coerce(body, ret, body_ty) {
            // Point at the value the body produces, not the whole block.
            let at = match &self.ast.exprs[body] {
                Expr::Block {
                    value: Some(value), ..
                } => *value,
                _ => body,
            };
            let from = func.returns.map(|t| self.ast.types.span(t).clone());
            self.mismatch_at(at, ret, body_ty, from);
        }
    }

    /// A property is read like a field, so it has to give a value.
    fn check_prop(&mut self, id: Id<Func>, func: &Func) {
        let ret = self.sigs[id.id].ret;
        if matches!(self.u.get(self.u.shallow(ret)), Ty::Unit) {
            let span = match func.returns {
                Some(ty) => self.ast.types.span(ty).clone(),
                None => func.name.span.clone(),
            };
            self.errors.push(TypeError::PropReturnsNothing {
                name: func.name.name.to_string(),
                span,
            });
        }
    }

    /// Reports integer literals too big for the type they ended up with: `let x: u8 = 300;`.
    /// Wrapping one silently is almost always a mistake; `as` asks for it.
    fn check_literals_fit(&mut self, types: &[TypeId]) {
        for (id, expr) in self.ast.exprs.iter() {
            let Expr::Int(n) = expr else { continue };
            let Ty::Int(int) = *self.u.table.get(types[id.id]) else {
                continue;
            };
            // `isize` and `usize` are as wide as a pointer: 64 bits is the widest they can be.
            let (min, max): (i128, u128) = match int {
                IntTy::I8 => (i8::MIN.into(), i8::MAX as u128),
                IntTy::I16 => (i16::MIN.into(), i16::MAX as u128),
                IntTy::I32 => (i32::MIN.into(), i32::MAX as u128),
                IntTy::I64 | IntTy::ISize => (i64::MIN.into(), i64::MAX as u128),
                IntTy::I128 => (i128::MIN, i128::MAX as u128),
                IntTy::U8 => (0, u8::MAX.into()),
                IntTy::U16 => (0, u16::MAX.into()),
                IntTy::U32 => (0, u32::MAX.into()),
                IntTy::U64 | IntTy::USize => (0, u64::MAX.into()),
                IntTy::U128 => (0, u128::MAX),
            };
            let fits = match *n < 0 {
                true => *n >= min,
                false => *n as u128 <= max,
            };
            if !fits {
                self.errors.push(TypeError::LiteralOutOfRange {
                    value: *n,
                    ty: format!("{int:?}").to_lowercase(),
                    max,
                    min,
                    span: self.span(id),
                    type_arg: false,
                });
            }
        }
    }

    /// Replaces all inference variables, producing the final tables.
    fn finish(mut self) -> (Typed, Vec<TypeError>) {
        let mut reported = HashSet::new();
        let error = self.u.table.error;
        // `@unreachable` where nothing decided a type: as a statement, it's `()`.
        for ty in std::mem::take(&mut self.unreachables) {
            if self.u.var_kind(ty) == Some(VarKind::Any) {
                let unit = self.u.table.unit;
                let _ = self.u.unify(ty, unit);
            }
        }

        let mut exprs = Vec::with_capacity(self.exprs.len());
        for (index, ty) in std::mem::take(&mut self.exprs).into_iter().enumerate() {
            let mut unresolved = None;
            let ty = match ty {
                Some(ty) => self.u.finish(ty, &mut unresolved),
                None => error,
            };
            if let Some(var) = unresolved
                && reported.insert(var)
            {
                let span = self.ast.exprs.span(Id::new(index)).clone();
                self.errors.push(TypeError::CannotInfer { span });
            }
            exprs.push(ty);
        }

        // Parameters that nothing constrains, like an unannotated, unused `x` in `fn f(x) {}`.
        for (func, sig) in self.ast.funcs.iter().zip(&self.sigs) {
            for (param, &ty) in func.1.params.iter().zip(&sig.params) {
                let mut unresolved = None;
                self.u.finish(ty, &mut unresolved);
                if let Some(var) = unresolved
                    && reported.insert(var)
                {
                    self.errors.push(TypeError::CannotInfer {
                        span: param.name.span.clone(),
                    });
                }
            }
        }

        // Anything else left unresolved was reported above.
        let mut ignore = None;
        let locals = std::mem::take(&mut self.locals)
            .into_iter()
            .map(|(def, ty)| (def, self.u.finish(ty, &mut ignore)))
            .collect();
        let funcs = std::mem::take(&mut self.sigs)
            .into_iter()
            .map(|sig| Signature {
                params: sig
                    .params
                    .into_iter()
                    .map(|p| self.u.finish(p, &mut ignore))
                    .collect(),
                ret: self.u.finish(sig.ret, &mut ignore),
            })
            .collect();
        let conversions = std::mem::take(&mut self.conversions)
            .into_iter()
            .map(|(expr, (def, args))| {
                let args = (args.into_iter())
                    .map(|a| self.u.finish(a, &mut ignore))
                    .collect();
                (expr, (def, args))
            })
            .collect();
        let type_args = std::mem::take(&mut self.type_args)
            .into_iter()
            .map(|(expr, args)| {
                (
                    expr,
                    args.into_iter()
                        .map(|a| self.u.finish(a, &mut ignore))
                        .collect(),
                )
            })
            .collect();

        let sliced = std::mem::take(&mut self.sliced)
            .into_iter()
            .map(|(expr, ty)| (expr, self.u.finish(ty, &mut ignore)))
            .collect();
        self.check_literals_fit(&exprs);
        let typed = Typed {
            table: self.u.table,
            exprs,
            locals,
            funcs,
            structs: self.structs,
            counts: self.counts,
            methods: self.methods,
            fields: self.fields,
            type_args,
            wrapped: self.wrapped,
            downgraded: self.downgraded,
            narrowed_reads: self.narrowed_reads,
            read_through: self.read_through,
            shared: self.shared,
            array_methods: self.array_methods,
            pointer_methods: self.pointer_methods,
            sliced,
            loops: self.for_loops,
            settled_ifs: self.settled_ifs,
            patterns: self.patterns,
            receivers: self.receivers,
            callees: self.callees,
            conversions,
            impls: self.impls,
            trait_ops: self.trait_ops,
            common: self.common,
            diverging: self.diverging,
        };
        (typed, self.errors)
    }

    // --- Types ---

    /// The type a type annotation describes.
    fn lower(&mut self, id: Id<TypeExpr>) -> TypeId {
        let ty = match &self.ast.types[id] {
            TypeExpr::Unit => Ty::Unit,
            // `str`, text before `string`: reported, and read as it.
            TypeExpr::Primitive(Primitive::Str) => {
                self.errors.push(TypeError::StrIsString {
                    span: self.ast.types.span(id).clone(),
                });
                return self.u.table.text;
            }
            TypeExpr::Primitive(p) => Ty::from_primitive(*p),
            TypeExpr::Pointer => Ty::Pointer,
            TypeExpr::PointerTo(to) => Ty::Ptr(self.lower(*to)),
            TypeExpr::Named { args, .. } => match self
                .names
                .ty(id)
                .map(|def| (def, self.names.def(def).kind.clone()))
            {
                Some((def, DefKind::Struct(s))) => {
                    let span = self.ast.types.span(id).clone();
                    Ty::Struct(def, self.lower_args(def, s, args, span))
                }
                // `N` in `struct Buffer<N: usize>` stands for a number, not a type; so does
                // a constant.
                Some((def, _))
                    if self.names.is_value_param(def)
                        || matches!(self.names.def(def).kind, DefKind::Const(_)) =>
                {
                    self.errors.push(TypeError::ValueAsType {
                        what: message::var(&*self.names.def(def).name).into(),
                        span: self.ast.types.span(id).clone(),
                    });
                    Ty::Error
                }
                Some((
                    def,
                    DefKind::TypeParam { .. }
                    | DefKind::StructParam { .. }
                    | DefKind::TraitParam { .. },
                )) => Ty::Param(def),
                Some((def, DefKind::ImplParam { .. })) => Ty::Param(def),
                Some((def, DefKind::AliasParam { .. })) => match self.alias_args.get(&def) {
                    Some(&ty) => return ty,
                    None => Ty::Param(def),
                },
                Some((_, DefKind::SelfType { impl_ })) => return self.self_type(impl_),
                Some((_, DefKind::ImplType { impl_, index })) => {
                    return self.impl_type(impl_, index);
                }
                Some((_, DefKind::AssocType { trait_, index })) => {
                    let base = self.names.projection_base(id);
                    return self.assoc_type(base, trait_, index);
                }
                Some((def, DefKind::Alias { ty })) => return self.lower_alias(id, def, ty, args),
                _ => Ty::Error, // already reported by the resolver
            },
            // `&str` from before strings were slices.
            TypeExpr::Reference { to, .. }
                if matches!(self.ast.types[*to], TypeExpr::Primitive(Primitive::Str)) =>
            {
                self.errors.push(TypeError::StrIsString {
                    span: self.ast.types.span(id).clone(),
                });
                return self.u.table.text;
            }
            TypeExpr::Reference { mutable, to, .. } => {
                let to = self.lower(*to);
                Ty::Ref {
                    mutable: *mutable,
                    to,
                }
            }
            TypeExpr::Optional(inner) => Ty::Optional(self.lower(*inner)),
            // `#T` is the standard library's `Rc<T>`; without it loaded, there's nothing it is.
            TypeExpr::Shared(inner) => {
                let inner = self.lower(*inner);
                match self.names.lang_struct(som_resolve::LangStruct::Rc) {
                    Some(rc) => Ty::Struct(rc, vec![inner]),
                    None => Ty::Error,
                }
            }
            TypeExpr::Array { element, len } => Ty::Array {
                element: self.lower(*element),
                len: self.lower_value(*len, None),
            },
            TypeExpr::Int(_) | TypeExpr::Bool(_) | TypeExpr::Binary { .. } => {
                self.errors.push(TypeError::ValueAsType {
                    what: message::code(self.ast.type_text(id)).into(),
                    span: self.ast.types.span(id).clone(),
                });
                Ty::Error
            }
            TypeExpr::Elements(element) => Ty::Slice(self.lower(*element)),
            TypeExpr::Slice { element, mutable } => {
                let element = self.lower(*element);
                let slice = self.u.intern(Ty::Slice(element));
                Ty::Ref {
                    mutable: *mutable,
                    to: slice,
                }
            }
            TypeExpr::Fn { params, returns } => {
                let params = params.iter().map(|&p| self.lower(p)).collect();
                let ret = match returns {
                    Some(r) => self.lower(*r),
                    None => self.u.table.unit,
                };
                Ty::Fn { params, ret }
            }
        };
        self.u.intern(ty)
    }

    /// What alias `def`, written at `id` with `args`, stands for: its type, `ty`, with its
    /// generic parameters standing for `args`.
    fn lower_alias(
        &mut self,
        id: Id<TypeExpr>,
        def: DefId,
        ty: Id<TypeExpr>,
        args: &[Id<TypeExpr>],
    ) -> TypeId {
        let params = self.names.alias_params(def).to_vec();
        let span = self.ast.types.span(id).clone();
        // `Vector3` in a `let` annotation: what its arguments are is worked out from the value.
        let args: Vec<TypeId> = if args.is_empty() && !params.is_empty() && self.omitted_args_ok {
            (0..params.len())
                .map(|_| self.u.fresh(VarKind::Any))
                .collect()
        } else if args.len() != params.len() {
            args.iter().for_each(|&a| {
                self.lower_arg(a, None);
            });
            self.errors.push(TypeError::WrongTypeArgCount {
                name: self.names.def(def).name.to_string(),
                expected: params.len(),
                found: args.len(),
                span,
            });
            return self.u.table.error;
        } else {
            (args.iter().zip(&params))
                .map(|(&arg, &param)| self.lower_arg(arg, Some(param)))
                .collect()
        };

        if self.expanding.contains(&def) {
            let start = self.expanding.iter().position(|&d| d == def).unwrap_or(0);
            // Report each alias in the cycle once, at its own definition.
            for &alias in &self.expanding[start..] {
                if self.cyclic.insert(alias) {
                    let alias = self.names.def(alias);
                    self.errors.push(TypeError::AliasCycle {
                        name: alias.name.to_string(),
                        span: alias.span.clone(),
                    });
                }
            }
            return self.u.table.error;
        }
        if self.cyclic.contains(&def) {
            return self.u.table.error;
        }
        let lowered = self.expand_alias(def, ty, &params, &args);
        if self.cyclic.contains(&def) {
            return self.u.table.error;
        }
        lowered
    }

    /// Alias `def`'s type, `ty`, with its parameters `params` standing for `args`.
    fn expand_alias(
        &mut self,
        def: DefId,
        ty: Id<TypeExpr>,
        params: &[DefId],
        args: &[TypeId],
    ) -> TypeId {
        let outer: Vec<_> = (params.iter().zip(args))
            .map(|(&param, &arg)| (param, self.alias_args.insert(param, arg)))
            .collect();
        // What it stands for is written out in full; nothing in it can be left out.
        let omitted = std::mem::take(&mut self.omitted_args_ok);
        self.expanding.push(def);
        let lowered = self.lower(ty);
        self.expanding.pop();
        self.omitted_args_ok = omitted;
        for (param, outer) in outer {
            match outer {
                Some(ty) => self.alias_args.insert(param, ty),
                None => self.alias_args.remove(&param),
            };
        }
        lowered
    }

    /// What alias `def` stands for where an expression names a struct through it,
    /// `Vector3::zero()`: its parameters each start unknown, for the value to decide.
    fn alias_value(&mut self, def: DefId) -> TypeId {
        let DefKind::Alias { ty } = self.names.def(def).kind else {
            return self.u.table.error;
        };
        if self.cyclic.contains(&def) {
            return self.u.table.error;
        }
        let params = self.names.alias_params(def).to_vec();
        let args: Vec<TypeId> = params.iter().map(|_| self.u.fresh(VarKind::Any)).collect();
        self.expand_alias(def, ty, &params, &args)
    }

    /// The type method `func`'s impl block is for, with the block's parameters standing for
    /// the first of `type_args`, what the method's were instantiated with.
    fn owner_type(&mut self, func: Id<Func>, type_args: &[TypeId]) -> Option<TypeId> {
        let index = self.names.impl_of(func)?;
        let info = self.impls.get(index)?;
        let (target, params) = (info.target?, info.params.clone());
        let args = type_args.get(..params.len())?;
        Some(crate::traits::substitute(
            &mut self.u.table,
            target,
            &params,
            args,
        ))
    }

    /// `Alloc<i32>::with_capacity(10)`: the type `func` is a function of, with the arguments
    /// written for it, has to be the type its impl block is for.
    fn fit_owner_args(
        &mut self,
        id: Id<Expr>,
        callee: &som_ast::Path,
        func: Id<Func>,
        type_args: &[TypeId],
        written: &[Id<TypeExpr>],
    ) {
        let def = self.names.func(func);
        if let Some(ty) = self.owner_written(id, callee, def, written)
            && let Some(target) = self.owner_type(func, type_args)
            && self.u.unify(target, ty).is_err()
        {
            self.mismatch(target, ty, self.span(id), None);
        }
    }

    /// The type written before the name of `def`, a function of it, in call `id`: `Alloc<i32>`
    /// in `Alloc<i32>::with_capacity(10)`. `None` if none is written, or, with an error, if
    /// it isn't a struct's function.
    fn owner_written(
        &mut self,
        id: Id<Expr>,
        callee: &som_ast::Path,
        def: DefId,
        written: &[Id<TypeExpr>],
    ) -> Option<TypeId> {
        if written.is_empty() {
            return None;
        }
        let DefKind::Func(func) = self.names.def(def).kind else {
            return None;
        };
        let owner = (self.names.impl_of(func))
            .and_then(|i| self.names.impls[i].owner)
            .filter(|&def| matches!(self.names.def(def).kind, DefKind::Struct(_)));
        let Some(owner) = owner else {
            written.iter().for_each(|&t| {
                self.lower_arg(t, None);
            });
            let path = &callee.segments[..callee.segments.len() - 1];
            self.errors.push(TypeError::WrongTypeArgCount {
                name: path.iter().map(|s| &*s.name).collect::<Vec<_>>().join("::"),
                expected: 0,
                found: written.len(),
                span: self.span(id),
            });
            return None;
        };
        let DefKind::Struct(s) = self.names.def(owner).kind else {
            unreachable!("filtered to structs")
        };
        let args = self.lower_args(owner, s, written, self.span(id));
        Some(self.u.intern(Ty::Struct(owner, args)))
    }

    /// `ty`, of expression `id`, has to be the type of the alias its path names a struct
    /// through, if it does: `Vector3::zero()` is a `Vector<3, f32>`.
    fn fit_alias(&mut self, id: Id<Expr>, ty: TypeId) {
        let Some(alias) = self.names.expr_alias(id) else {
            return;
        };
        let aliased = self.alias_value(alias);
        if self.u.unify(aliased, ty).is_err() {
            self.mismatch(aliased, ty, self.span(id), None);
        }
    }

    /// The arguments written for struct `s`, `def`: each a type, or a value for a value
    /// parameter. If there are too many or too few, an error for each.
    fn lower_args(
        &mut self,
        def: DefId,
        s: som_ast::Id<som_ast::Struct>,
        args: &[Id<TypeExpr>],
        span: Span,
    ) -> Vec<TypeId> {
        let params = self.names.struct_params(s);
        // `List` in a `let` annotation: what they are is worked out from the value.
        if args.is_empty() && !params.is_empty() && self.omitted_args_ok {
            return (0..params.len())
                .map(|_| self.u.fresh(VarKind::Any))
                .collect();
        }
        let params = params.to_vec();
        let defaults: Vec<Option<Id<TypeExpr>>> = (self.ast.structs[s].generics.iter())
            .map(|g| g.default)
            .collect();
        let required = defaults.iter().take_while(|d| d.is_none()).count();
        if args.len() < required || args.len() > params.len() {
            args.iter().for_each(|&a| {
                self.lower_arg(a, None);
            });
            self.wrong_arg_count(def, required, params.len(), args.len(), span);
            return vec![self.u.table.error; params.len()];
        }
        let given: Vec<TypeId> = (args.iter().zip(&params))
            .map(|(&arg, &param)| self.lower_arg(arg, Some(param)))
            .collect();
        self.with_defaults(&params, &defaults, given)
    }

    /// `given`, the arguments written for generic parameters `params`, then the rest from
    /// their `defaults`, which can name the parameters before them: `B` in
    /// `struct Pair<A, B = A>` is `A`'s argument if it's left out.
    pub(super) fn with_defaults(
        &mut self,
        params: &[DefId],
        defaults: &[Option<Id<TypeExpr>>],
        mut given: Vec<TypeId>,
    ) -> Vec<TypeId> {
        for index in given.len()..params.len() {
            let Some(default) = defaults.get(index).copied().flatten() else {
                given.push(self.u.table.error);
                continue;
            };
            let ty = self.lower(default);
            let subst: Vec<(DefId, TypeId)> =
                params.iter().copied().zip(given.iter().copied()).collect();
            let ty = self.substitute(ty, &subst);
            given.push(ty);
        }
        given
    }

    /// Reports `found` type arguments for `def`, which takes `required` to `most`.
    pub(super) fn wrong_arg_count(
        &mut self,
        def: DefId,
        required: usize,
        most: usize,
        found: usize,
        span: Span,
    ) {
        let name = self.names.def(def).name.to_string();
        self.errors.push(match required == most {
            true => TypeError::WrongTypeArgCount {
                name,
                expected: most,
                found,
                span,
            },
            false => TypeError::WrongTypeArgRange {
                name,
                required,
                most,
                found,
                span,
            },
        });
    }

    /// An argument for generic parameter `param`: a type, or a value if it's a value parameter.
    fn lower_arg(&mut self, arg: Id<TypeExpr>, param: Option<DefId>) -> TypeId {
        match param.and_then(|p| self.value_params.get(&p).copied()) {
            Some(ty) => self.lower_value(arg, Some(ty)),
            None => self.lower(arg),
        }
    }

    /// The number `n`, written where a type argument goes, of type `expected` or of any
    /// integer type for an array's length.
    fn lower_number(&mut self, n: u128, expected: Option<TypeId>, span: Span) -> TypeId {
        let error = self.u.table.error;
        let int = expected.unwrap_or(self.u.table.usize);
        match self.u.get(int).clone() {
            Ty::Int(int_ty) => {
                let max: u128 = match int_ty {
                    IntTy::I8 => i8::MAX as u128,
                    IntTy::I16 => i16::MAX as u128,
                    IntTy::I32 => i32::MAX as u128,
                    IntTy::I64 | IntTy::ISize => i64::MAX as u128,
                    IntTy::I128 => i128::MAX as u128,
                    IntTy::U8 => u8::MAX as u128,
                    IntTy::U16 => u16::MAX as u128,
                    IntTy::U32 => u32::MAX as u128,
                    IntTy::U64 | IntTy::USize => u64::MAX as u128,
                    IntTy::U128 => u128::MAX,
                };
                if n > max {
                    self.errors.push(TypeError::LiteralOutOfRange {
                        value: n.min(i128::MAX as u128) as i128,
                        ty: format!("{int_ty:?}").to_lowercase(),
                        max,
                        min: 0,
                        span,
                        type_arg: true,
                    });
                    return error;
                }
                return self.u.intern(Ty::Value(Value::Int(n)));
            }
            _ => {
                let found = self.u.fresh(VarKind::Int);
                if self.u.unify(int, found).is_err() {
                    self.mismatch(int, found, span, None);
                    return error;
                }
                self.u.intern(Ty::Value(Value::Int(n)))
            }
        }
    }

    fn const_number(&self, expr: Id<Expr>) -> Option<i128> {
        crate::const_number(self.ast, self.names, expr)
    }

    /// A value written where a type argument goes, of type `expected`, or of any integer type
    /// for an array's length: a literal, or a value parameter.
    fn lower_value(&mut self, id: Id<TypeExpr>, expected: Option<TypeId>) -> TypeId {
        let span = self.ast.types.span(id).clone();
        let error = self.u.table.error;
        let (value, found) = match self.ast.types[id] {
            TypeExpr::Int(n) => return self.lower_number(n, expected, span),
            // `[i32; W]` of `@const W = 60;`: the number it is.
            TypeExpr::Named { ref args, .. }
                if args.is_empty()
                    && let Some(def) = self.names.ty(id)
                    && let DefKind::Const(c) = self.names.def(def).kind =>
            {
                return match self.const_number(self.ast.consts[c].value) {
                    Some(n) if n >= 0 => self.lower_number(n as u128, expected, span),
                    None if som_resolve::const_runs_code(self.ast, self.names, c) => {
                        self.errors.push(TypeError::ConstNeededTooEarly {
                            name: self.names.def(def).name.to_string(),
                            span,
                            defined_at: self.names.def(def).span.clone(),
                        });
                        error
                    }
                    _ => {
                        self.errors.push(TypeError::ConstNotCount {
                            name: self.names.def(def).name.to_string(),
                            span,
                            defined_at: self.names.def(def).span.clone(),
                        });
                        error
                    }
                };
            }
            TypeExpr::Bool(b) => (Ty::Value(Value::Bool(b)), self.u.table.bool),
            // `A + B`: arithmetic on numbers, of the same type.
            TypeExpr::Binary { op, lhs, rhs } => {
                if let Some(ty) = expected
                    && !matches!(self.u.get(ty), Ty::Int(_) | Ty::Error)
                {
                    let found = self.u.fresh(VarKind::Int);
                    self.mismatch(ty, found, span, None);
                    return error;
                }
                let (lhs, rhs) = (
                    self.lower_value(lhs, expected),
                    self.lower_value(rhs, expected),
                );
                if lhs == error || rhs == error {
                    return error;
                }
                let ty = match op {
                    BinOp::Multiply => Ty::Mul(lhs, rhs),
                    _ => Ty::Add(lhs, rhs),
                };
                let ty = self.u.intern(ty);
                return self.u.normalize(ty);
            }
            // A value parameter of an alias being expanded: the value it stands for.
            TypeExpr::Named { ref args, .. }
                if args.is_empty()
                    && let Some(def) = self.names.ty(id)
                    && let Some(&value) = self.alias_args.get(&def) =>
            {
                return value;
            }
            TypeExpr::Named { ref args, .. }
                if args.is_empty()
                    && let Some(def) = self.names.ty(id)
                    && let Some(&ty) = self.value_params.get(&def) =>
            {
                (Ty::Param(def), ty)
            }
            // Already reported by the resolver.
            TypeExpr::Named { .. } if self.names.ty(id).is_none() => return error,
            _ => {
                let what = message::code(self.ast.type_text(id)).into();
                let ty = expected.map(|ty| self.describe(ty));
                self.errors.push(TypeError::TypeAsValue { what, ty, span });
                return error;
            }
        };
        // An array's length can be any integer.
        let fits = match expected {
            Some(expected) => self.u.unify(expected, found).is_ok(),
            None => {
                matches!(self.u.get(found), Ty::Int(_) | Ty::Error)
                    || self.u.var_kind(found) == Some(VarKind::Int)
            }
        };
        if !fits {
            let expected = expected.unwrap_or(self.u.table.usize);
            self.mismatch(expected, found, span, None);
            return error;
        }
        self.u.intern(value)
    }

    /// What each of struct `s`'s type parameters stands for: `args`.
    fn struct_subst(
        &self,
        s: som_ast::Id<som_ast::Struct>,
        args: &[TypeId],
    ) -> Vec<(DefId, TypeId)> {
        let params = self.names.struct_params(s);
        params.iter().copied().zip(args.iter().copied()).collect()
    }

    /// A function's signature with its type parameters replaced by fresh variables.
    fn instantiate(&mut self, func: Id<Func>) -> (Signature, Vec<TypeId>) {
        // A return type that isn't written, or a property's that leaves out arguments, is what
        // the body decides: check it first.
        if self.ast.funcs[func].returns.is_none() || self.ast.funcs[func].prop {
            self.check_func(func);
        }
        let mut sig = self.sigs[func.id].clone();
        let type_params = self.names.type_params(func);
        if type_params.is_empty() {
            return (sig, Vec::new());
        }
        // Still undecided: called from inside its own body, or one it's being checked for.
        // The call works its type out on its own, rather than deciding the body's for every
        // instantiation.
        if matches!(self.u.get(self.u.shallow(sig.ret)), Ty::Var(_)) {
            sig.ret = self.u.fresh(VarKind::Any);
        }
        let args: Vec<TypeId> = type_params
            .iter()
            .map(|_| self.u.fresh(VarKind::Any))
            .collect();
        let subst: Vec<(DefId, TypeId)> = type_params
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect();
        let sig = Signature {
            params: sig
                .params
                .iter()
                .map(|&p| self.substitute(p, &subst))
                .collect(),
            ret: self.substitute(sig.ret, &subst),
        };
        (sig, args)
    }

    /// The generic parameters type arguments written for a call of `func` stand for: its own,
    /// after its impl block's; or if it has none, its impl block's, which are the type's.
    /// `Alloc::new<u8>()` is a `new` of `Alloc<u8>`.
    fn own_type_params(&self, func: Id<Func>) -> Vec<DefId> {
        let all = self.names.type_params(func);
        let inherited = (self.names.impl_of(func))
            .map_or(0, |i| self.names.impls[i].params.len())
            .min(all.len());
        if inherited == all.len() {
            return all.to_vec();
        }
        all[inherited..].to_vec()
    }

    /// `null<i32>()`: the type arguments written for a call of `func`, which stand for its own
    /// generic parameters, whose fresh variables are the last of `type_args`.
    fn written_type_args(
        &mut self,
        call: Id<Expr>,
        func: Id<Func>,
        written: &[Id<TypeExpr>],
        type_args: &[TypeId],
    ) {
        let own = self.own_type_params(func);
        if written.len() != own.len() {
            written.iter().for_each(|&a| {
                self.lower_arg(a, None);
            });
            self.errors.push(TypeError::WrongTypeArgCount {
                name: self.ast.funcs[func].name.name.to_string(),
                expected: own.len(),
                found: written.len(),
                span: self.span(call),
            });
            return;
        }
        let inherited = type_args.len() - own.len();
        for ((&arg, &param), &var) in written.iter().zip(&own).zip(&type_args[inherited..]) {
            let ty = self.lower_arg(arg, Some(param));
            let _ = self.u.unify(var, ty);
        }
    }

    fn substitute(&mut self, ty: TypeId, subst: &[(DefId, TypeId)]) -> TypeId {
        // An unwritten return type is a variable its body decided: what it was decided to be.
        let ty = self.u.shallow(ty);
        match self.u.get(ty).clone() {
            Ty::Param(def) => subst
                .iter()
                .find(|(d, _)| *d == def)
                .map_or(ty, |(_, t)| *t),
            other if other.children().is_empty() => ty,
            other => {
                let substituted = other.map_children(|child| self.substitute(child, subst));
                let substituted = self.u.intern(substituted);
                // `N + 1` with `N` filled in may add up: `3 + 1` is `4`; and `I::Item` with
                // `I` filled in may be known.
                let substituted = self.u.normalize(substituted);
                self.normalize_assoc(substituted)
            }
        }
    }

    /// Reports arithmetic in types that turned out unequal once more was known, like a
    /// `[T; R * C]` that's `[T; 6]` with `R` and `C` both 2. Where's no longer known, so it's
    /// reported at `span`, the function it was found in.
    fn settle(&mut self, span: Span) {
        for (expected, found) in self.u.settle() {
            self.mismatch(expected, found, span.clone(), None);
        }
    }

    // --- Errors ---

    /// A type for an error message. A type that isn't known yet can't be written down, so
    /// it's described in plain words instead: "an integer", not a type called that.
    fn describe(&self, ty: TypeId) -> Message {
        let text = match self.u.var_kind(ty) {
            Some(VarKind::Int) => "an integer".to_string(),
            Some(VarKind::Float) => "a float".to_string(),
            Some(VarKind::Text) => "a string".to_string(),
            Some(VarKind::Collection { len, .. })
                if *self.u.get(self.u.shallow(len)) == Ty::Value(Value::Int(1)) =>
            {
                "a collection of 1 element".to_string()
            }
            Some(VarKind::Collection { len, .. }) => {
                format!(
                    "a collection of {} elements",
                    self.u.display(len, self.names)
                )
            }
            Some(VarKind::Any) => "an unknown type".to_string(),
            None => return message::ty(self.u.display(ty, self.names)).into(),
        };
        text.into()
    }

    fn mismatch(
        &mut self,
        expected: TypeId,
        found: TypeId,
        span: Span,
        expected_from: Option<Span>,
    ) {
        let optional = self
            .unexpected_optional(expected, found)
            .then(|| (span.get_text().to_string(), false));
        self.errors.push(TypeError::Mismatch {
            expected: self.describe(expected),
            found: self.describe(found),
            span,
            expected_from,
            fix: None,
            optional,
        });
    }

    /// Like `mismatch`, for the value of `expr`: a `&T` where a `T` is expected gets a fix
    /// dereferencing it.
    fn mismatch_at(
        &mut self,
        expr: Id<Expr>,
        expected: TypeId,
        found: TypeId,
        expected_from: Option<Span>,
    ) {
        if let Some(error) = self.not_borrowed(expr, expected, found, expected_from.clone()) {
            self.errors.push(error);
            return;
        }
        // `fflush(none)`: a null pointer is `mem::null()`; `none` is an optional's.
        if matches!(self.ast.exprs[expr], Expr::None)
            && matches!(self.u.get(self.u.shallow(expected)), Ty::Pointer | Ty::Ptr(_))
        {
            self.errors.push(TypeError::NoneAsPointer {
                ty: self.describe(expected),
                span: self.span(expr),
                expected_from,
            });
            // Reported: not also that its type can't be inferred.
            self.exprs[expr.id] = Some(self.u.table.error);
            return;
        }
        let fix = (self.deref_fix(expr, expected, found))
            .or_else(|| self.fallback_fix(expr, expected, found));
        let span = self.span(expr);
        let optional = self.unexpected_optional(expected, found).then(|| {
            let variable = matches!(self.ast.exprs[expr], Expr::Var(_));
            (span.get_text().to_string(), variable)
        });
        self.errors.push(TypeError::Mismatch {
            expected: self.describe(expected),
            found: self.describe(found),
            span,
            expected_from,
            fix,
            optional,
        });
    }

    /// An optional `expr` of type `found` where what it holds, `expected`, is: a fallback for
    /// `none`, where there's an obvious one. `x else 0`, or for a reference to a copy,
    /// `*(nums.get(0) else &0)`.
    fn fallback_fix(&self, expr: Id<Expr>, expected: TypeId, found: TypeId) -> Option<Fix> {
        let Ty::Optional(inner) = *self.u.get(self.u.shallow(found)) else {
            return None;
        };
        let literal = |ty: TypeId| {
            let ty = self.u.shallow(ty);
            match (self.u.get(ty), self.u.var_kind(ty)) {
                (_, Some(VarKind::Int)) | (Ty::Int(_), _) => Some("0"),
                (_, Some(VarKind::Float)) | (Ty::Float(_), _) => Some("0.0"),
                (Ty::Bool, _) => Some("false"),
                _ => None,
            }
        };
        let span = self.span(expr);
        let text = span.get_text();
        if text.contains('\n') {
            return None;
        }
        let replacement = if self.u.could_fit(expected, inner) {
            let fallback = format!("{text} else {}", literal(inner)?);
            // `else` binds loosest: parentheses unless it stands alone, as `let`'s value or
            // an argument.
            if stands_alone(&span) {
                fallback
            } else {
                format!("({fallback})")
            }
        } else if let Ty::Ref { to, .. } = *self.u.get(self.u.shallow(inner))
            && self.u.could_fit(expected, to)
            && self.is_copy(to)
        {
            format!("*({text} else &{})", literal(to)?)
        } else {
            return None;
        };
        Some(Fix::replace(
            msg![
                "give a fallback for ",
                message::code("none"),
                ": ",
                message::code(&replacement)
            ],
            span,
            replacement,
        ))
    }

    /// An optional, `found`, where `expected` isn't one: what it holds has to be got at.
    fn unexpected_optional(&self, expected: TypeId, found: TypeId) -> bool {
        let is_optional = |ty| matches!(self.u.get(self.u.shallow(ty)), Ty::Optional(_));
        is_optional(found) && !is_optional(expected)
    }

    /// `expr`, a value, where a reference to one is expected: `Hash::hash(key, …)` for
    /// `&self`. The reference type is written from the value's, `&K`, as the call may not
    /// have worked out what it refers to; and the fix borrows it, `&key` or `&mut key`.
    fn not_borrowed(
        &self,
        expr: Id<Expr>,
        expected: TypeId,
        found: TypeId,
        expected_from: Option<Span>,
    ) -> Option<TypeError> {
        // A reference, or an optional one, `&V?`, which a reference is wrapped into.
        let shallow = self.u.shallow(expected);
        let (reference, optional) = match *self.u.get(shallow) {
            Ty::Optional(inner) => (self.u.shallow(inner), true),
            _ => (shallow, false),
        };
        let Ty::Ref { mutable, to } = *self.u.get(reference) else {
            return None;
        };
        if matches!(
            self.u.get(self.u.shallow(found)),
            Ty::Ref { .. } | Ty::Optional(_) | Ty::Error
        ) || !self.u.could_fit(to, found)
        {
            return None;
        }
        let borrow = if mutable { "&mut " } else { "&" };
        let found_text = self.u.display(found, self.names);
        let question = if optional { "?" } else { "" };
        let span = self.span(expr);
        // `&a.b`, `&f()` and `&0` borrow as they are; anything looser, in parentheses.
        let edits = if self.binds_tightly(expr) {
            vec![(span.before(), borrow.to_string())]
        } else {
            vec![
                (span.before(), format!("{borrow}(")),
                (span.after(), ")".to_string()),
            ]
        };
        Some(TypeError::NotBorrowed {
            expected: message::code(format!("{borrow}{found_text}{question}")).into(),
            field: matches!(self.ast.exprs[expr], Expr::Field { .. }),
            found: self.describe(found),
            span: span.clone(),
            expected_from,
            fix: Fix {
                title: msg![
                    "borrow it: ",
                    message::code(format!("{borrow}{}", span.get_text()))
                ],
                edits,
            },
        })
    }

    /// A fix for `expr`, a reference to what's expected. A value that copies is dereferenced,
    /// `*expr`. A type parameter is required to copy, `T: Copy`: for a loop's variable that's
    /// all it takes, as the loop then copies its values, and anything else is dereferenced
    /// too. A value that moves can't be taken out of a reference at all: no fix.
    /// Whether `expr` can take a prefix, `&` or `*`, without parentheses: a name, what's
    /// read from one, a call, a literal.
    fn binds_tightly(&self, expr: Id<Expr>) -> bool {
        matches!(
            self.ast.exprs[expr],
            Expr::Var(_)
                | Expr::Field { .. }
                | Expr::Index { .. }
                | Expr::Call { .. }
                | Expr::MethodCall { .. }
                | Expr::Deref(_)
                | Expr::Int(_)
                | Expr::Float(_)
                | Expr::Bool(_)
                | Expr::Str(_)
                | Expr::Array(_)
        )
    }

    fn deref_fix(&self, expr: Id<Expr>, expected: TypeId, found: TypeId) -> Option<Fix> {
        let Ty::Ref { to, .. } = *self.u.get(self.u.shallow(found)) else {
            return None;
        };
        if !self.u.could_fit(expected, to) || self.u.could_fit(expected, found) {
            return None;
        }
        let param = matches!(self.u.get(self.u.shallow(to)), Ty::Param(_));
        if !self.is_copy(to) && !param {
            return None;
        }
        let span = self.span(expr);
        // A name, or what's read from one, takes a `*` as it is; anything else in parentheses.
        let deref = if self.binds_tightly(expr) {
            vec![(span.before(), "*".to_string())]
        } else {
            vec![
                (span.before(), "*(".to_string()),
                (span.after(), ")".to_string()),
            ]
        };
        let deref_title = msg![
            "dereference it: ",
            message::code(format!("*{}", span.get_text()))
        ];
        if !param || self.is_copy(to) {
            return Some(Fix {
                title: deref_title,
                edits: deref,
            });
        }
        let bound = self.operator_fix(to, LangTrait::Copy, None)?;
        if self.lent_loop_variable(expr) {
            return Some(bound);
        }
        let mut edits = bound.edits;
        edits.extend(deref);
        Some(Fix {
            title: msg![bound.title, ", and ", deref_title],
            edits,
        })
    }

    /// Whether `expr` is the variable of a `for` loop that lends its values because they may
    /// move: one that would copy them if they're required to copy.
    fn lent_loop_variable(&self, expr: Id<Expr>) -> bool {
        let Expr::Var(_) = self.ast.exprs[expr] else {
            return false;
        };
        let Some(def) = self.names.expr(expr) else {
            return false;
        };
        let DefKind::Local(stmt) = self.names.def(def).kind else {
            return false;
        };
        matches!(
            self.for_loops.get(&stmt),
            Some(Loop::Elements { take: Take::Ref })
                | Some(Loop::Iterator {
                    start: crate::Start::Iter,
                    copied: false
                })
        )
    }

    fn invalid_operands(&mut self, expr: Id<Expr>, op: BinOp, lhs: TypeId, rhs: TypeId) {
        // An optional operand is the usual reason: say how to get at its value.
        let optional = match self.ast.exprs[expr] {
            Expr::Binary {
                lhs: lhs_expr,
                rhs: rhs_expr,
                ..
            } => [(lhs_expr, lhs), (rhs_expr, rhs)]
                .into_iter()
                .find_map(|(e, ty)| self.optional_operand(e, ty)),
            _ => None,
        };
        // A struct's operators are its traits': say which to implement, or for a type
        // parameter, to require.
        let implement = LangTrait::of_operator(op).and_then(|lang| {
            let ty = self.u.get(self.u.shallow(lhs));
            match ty {
                Ty::Param(def) => Some((lang.name(), Some(self.names.def(*def).name.to_string()))),
                Ty::Var(_) | Ty::Error | Ty::Optional(_) => None,
                _ if self.has_builtin(lhs, lang) => None,
                // `string`'s operators are the standard library's to give.
                Ty::Struct(def, _) if self.in_std(*def) => None,
                _ => Some((lang.name(), None)),
            }
        });
        let fix = LangTrait::of_operator(op)
            .filter(|_| implement.is_some())
            .and_then(|lang| self.operator_fix(lhs, lang, Some(rhs)))
            .or_else(|| self.interpolation_fix(expr, op, lhs, rhs));
        let takes = LangTrait::of_operator(op)
            .map(|lang| self.operator_takes(lhs, lang))
            .unwrap_or_default();
        self.errors.push(TypeError::InvalidOperands {
            op: op.symbol().into(),
            lhs: self.describe(lhs),
            rhs: self.describe(rhs),
            span: self.ast.exprs.span(expr).clone(),
            optional,
            implement,
            fix,
            takes,
        });
    }

    /// `"score: " + n`, text and something that isn't: written into the text instead,
    /// `"score: {n}"`, as `+` only joins text.
    fn interpolation_fix(
        &self,
        expr: Id<Expr>,
        op: BinOp,
        lhs: TypeId,
        rhs: TypeId,
    ) -> Option<Fix> {
        let Expr::Binary {
            lhs: left,
            rhs: right,
            ..
        } = self.ast.exprs[expr]
        else {
            return None;
        };
        let text = self.u.table.text;
        let is_text = |c: &Self, ty: TypeId| {
            c.u.shallow(ty) == text || c.u.var_kind(ty) == Some(VarKind::Text)
        };
        if op != BinOp::Add || is_text(self, lhs) == is_text(self, rhs) {
            return None;
        }
        // Each side: a literal's text as it is, anything else as a value written in.
        let side = |c: &Self, e: Id<Expr>| -> Option<String> {
            let written = c.span(e).get_text();
            match c.ast.exprs[e] {
                Expr::Str(_) => Some(written[1..written.len() - 1].to_string()),
                _ if written.contains(['"', '\n', '{', '}']) => None,
                _ => Some(format!("{{{written}}}")),
            }
        };
        let joined = format!("\"{}{}\"", side(self, left)?, side(self, right)?);
        Some(Fix::replace(
            msg!["write it into the text: ", message::code(joined.clone())],
            self.span(expr),
            joined,
        ))
    }

    /// For an operand `expr` of optional type `ty`: how it's written, a fallback value to
    /// suggest for it, and where it is.
    fn optional_operand(&self, expr: Id<Expr>, ty: TypeId) -> Option<(String, String, Span)> {
        let Ty::Optional(inner) = *self.u.get(self.u.shallow(ty)) else {
            return None;
        };
        let text = self.span(expr).get_text().to_string();
        if text.len() > 30 || text.contains('\n') {
            return None;
        }
        let fallback = match (self.u.get(self.u.shallow(inner)), self.u.var_kind(inner)) {
            (_, Some(VarKind::Int)) | (Ty::Int(_), _) => "0",
            (_, Some(VarKind::Float)) | (Ty::Float(_), _) => "0.0",
            _ => "…",
        };
        Some((text, fallback.to_string(), self.span(expr)))
    }

    fn span(&self, expr: Id<Expr>) -> Span {
        self.ast.exprs.span(expr).clone()
    }

    // --- Visibility ---

    /// Who's looking from `at`: its module, its file, and the type whose impl block it's in.
    fn viewer<'s>(&self, at: &'s Span) -> Viewer<'s> {
        let file = at.source.identifier();
        let module = (self.ast.modules.iter())
            .position(|m| m.files.iter().any(|f| f.identifier() == file))
            .unwrap_or(usize::MAX);
        let owner = (self.current_func)
            .and_then(|f| self.names.impl_of(f))
            .and_then(|i| self.names.impls[i].owner);
        Viewer {
            module,
            file,
            owner,
        }
    }

    /// Reports `name`, used at `span`, if it's hidden from there.
    fn report_hidden(&mut self, name: &str, span: &Span, hidden: Option<Hidden>) {
        if let Some(hidden) = hidden {
            self.errors.push(TypeError::NotVisible {
                name: name.to_string(),
                span: span.clone(),
                hidden,
            });
        }
    }

    /// Reports method or property `def` if it can't be seen at `span`, where it's used.
    fn check_visible(&mut self, def: DefId, span: &Span) {
        let hidden = visibility::hidden(self.ast, self.names, def, self.viewer(span));
        self.report_hidden(&self.names.def(def).name.clone(), span, hidden);
    }

    /// The functions of struct `def` that code at `span` can call to make one, like
    /// `Hello::new` or `List::with_capacity`: those without a `self` that return it. Each as
    /// a fix replacing `Hello { … }` at `span` with a call to it, its arguments the values
    /// `given` to fields of the same name, or else the parameters' names.
    fn makers(
        &self,
        def: DefId,
        path: &str,
        given: &[(som_ast::Ident, Id<Expr>)],
        span: &Span,
    ) -> Vec<Fix> {
        let viewer = self.viewer(span);
        let mut fixes = Vec::new();
        for (name, method) in self.names.methods_of(def) {
            let DefKind::Func(f) = self.names.def(method).kind else {
                continue;
            };
            let params = &self.ast.funcs[f].params;
            if params.first().is_some_and(|p| &*p.name.name == "self")
                || !visibility::can_see(self.ast, self.names, method, viewer)
            {
                continue;
            }
            // A `Self`, or a `Self?`: `File::open` makes one if it can.
            let mut ret = self.u.shallow(self.sigs[f.id].ret);
            if let Ty::Optional(inner) = *self.u.get(ret) {
                ret = self.u.shallow(inner);
            }
            if !matches!(self.u.get(ret), Ty::Struct(d, _) if *d == def) {
                continue;
            }
            let args: Vec<String> = (params.iter())
                .map(|p| {
                    (given.iter())
                        .find(|(field, _)| field.name == p.name.name)
                        .map_or(p.name.name.to_string(), |(_, value)| {
                            self.span(*value).get_text().to_string()
                        })
                })
                .collect();
            let call = format!("{path}::{name}");
            fixes.push(Fix {
                title: msg!["call ", message::code(call.clone())],
                edits: vec![(span.clone(), format!("{call}({})", args.join(", ")))],
            });
        }
        fixes
    }

    /// The field of a value of type `object_ty` called `field` that a property of its name
    /// stands in for, where the field can't be seen: its type's name, and why it's hidden.
    fn hidden_field_named(
        &self,
        object_ty: TypeId,
        field: &som_ast::Ident,
    ) -> Option<(String, Hidden)> {
        let mut ty = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
        }
        let Ty::Struct(def, _) = self.u.get(ty) else {
            return None;
        };
        let DefKind::Struct(s) = self.names.def(*def).kind else {
            return None;
        };
        let strukt = &self.ast.structs[s];
        let i = (!strukt.is_enum())
            .then(|| strukt.fields.iter().position(|f| f.name.name == field.name))??;
        let hidden =
            visibility::hidden_field(self.ast, self.names, s, i, self.viewer(&field.span))?;
        Some((self.names.def(*def).name.to_string(), hidden))
    }

    /// Whether a value of type `object_ty`, maybe through a reference, is a struct with a
    /// field called `field` that can be seen where it's written.
    fn field_seen(&self, object_ty: TypeId, field: &som_ast::Ident) -> bool {
        let mut ty = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
        }
        let Ty::Struct(def, _) = self.u.get(ty) else {
            return false;
        };
        let DefKind::Struct(s) = self.names.def(*def).kind else {
            return false;
        };
        let strukt = &self.ast.structs[s];
        if strukt.is_enum() {
            return false;
        }
        (strukt.fields.iter())
            .position(|f| f.name.name == field.name)
            .is_some_and(|i| {
                let viewer = self.viewer(&field.span);
                visibility::hidden_field(self.ast, self.names, s, i, viewer).is_none()
            })
    }

    /// Reports field `index` of struct `s` if it can't be seen at `span`, where it's used.
    fn check_field_visible(&mut self, s: Id<som_ast::Struct>, index: usize, span: &Span) {
        let hidden = visibility::hidden_field(self.ast, self.names, s, index, self.viewer(span));
        let name = self.ast.structs[s].fields[index].name.name.clone();
        self.report_hidden(&name, span, hidden);
    }

    // --- Statements ---

    fn check_stmts(&mut self, stmts: &[Id<Stmt>]) {
        let outer: HashSet<usize> = self.narrowed.iter().map(|n| n.id).collect();
        for &stmt in stmts {
            self.check_stmt(stmt);
        }
        // What an assignment in here narrowed ends with the block; what ended stays ended.
        self.narrowed.retain(|n| outer.contains(&n.id));
    }

    /// A loop's `body`.
    fn check_loop(&mut self, body: &[Id<Stmt>]) {
        // A later iteration sees what the body changes, even further down.
        self.forget_changed_in(body);
        self.loops += 1;
        self.check_stmts(body);
        self.loops -= 1;
    }

    fn check_stmt(&mut self, id: Id<Stmt>) {
        match &self.ast.stmts[id] {
            Stmt::Use(_) => {}
            // Checked on its own, apart from the function around it.
            Stmt::Const(c) => {
                self.check_const(*c);
            }
            Stmt::Let { ty, value, .. } => {
                let ty = match ty {
                    Some(annotation) => {
                        self.omitted_args_ok = true;
                        let expected = self.lower(*annotation);
                        self.omitted_args_ok = false;
                        let from = self.ast.types.span(*annotation).clone();
                        self.expect(*value, expected, Some(from));
                        expected
                    }
                    // `let a = "bob"`: a `string`, whatever it's used as later. Only text
                    // passed straight to C becomes a `cstr`.
                    None => {
                        let ty = self.infer(*value);
                        self.text_as_string(ty);
                        ty
                    }
                };
                if let Some(def) = self.names.local(id) {
                    self.locals.insert(def, ty);
                    let key = Key {
                        root: def,
                        path: Vec::new(),
                    };
                    self.narrow_if_assigned_a_value(Some(key), ty, *value);
                }
            }
            Stmt::Expr(expr) => {
                // A `match` whose value isn't used can have arms of different types.
                self.discarding = matches!(self.ast.exprs[*expr], Expr::Match { .. });
                let before = self.errors.len();
                let ty = self.infer(*expr);
                // `{ begin(); end() }` of a loop or `if`: the last call, without its `;`.
                if self.ast.unterminated.contains(&id)
                    && matches!(
                        self.ast.exprs[*expr],
                        Expr::Call { .. } | Expr::MethodCall { .. }
                    )
                {
                    self.tail_calls.push((*expr, ty));
                }
                self.discarding = false;
                // Once what's wrong with it is fixed, it may not be what's meant either.
                if self.errors.len() == before {
                    self.check_unused(*expr);
                }
            }
            Stmt::Loop { body } => self.check_loop(body),
            Stmt::While { condition, body } => {
                // The condition runs again after the body.
                self.forget_changed_in(body);
                let bool = self.u.table.bool;
                self.expect(*condition, bool, None);
                self.check_loop(body);
            }
            // `@for i in 0..4`: copied once for each value, so they have to be known.
            Stmt::For { iterable, .. }
                if self.ast.settled.contains(&id) && !self.settled_iterable(*iterable) =>
            {
                self.errors.push(TypeError::NotSettled {
                    what: "what an `@for` loops over",
                    hint: "loop over a range of literals, constants and a generic function's \
                           value parameters, or over an array written out",
                    span: self.span(*iterable),
                });
                self.infer(*iterable);
            }
            Stmt::For { iterable, body, .. } => {
                // A later iteration sees what the body changes.
                self.forget_changed_in(body);
                let ty = self.infer(*iterable);
                let element = self.loop_element(id, *iterable, ty);
                if let Some(def) = self.names.local(id) {
                    self.locals.insert(def, element);
                }
                self.check_loop(body);
            }
            Stmt::Return(value) => {
                let (ret, from) = self
                    .returns
                    .clone()
                    .expect("statements are inside a function");
                match value {
                    Some(value) => self.expect(*value, ret, from),
                    None => {
                        let unit = self.u.table.unit;
                        if self.u.unify(ret, unit).is_err() {
                            self.errors.push(TypeError::ReturnWithoutValue {
                                ty: self.describe(ret),
                                span: self.ast.stmts.span(id).clone(),
                            });
                        }
                    }
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) if self.loops == 0 => {
                self.errors.push(TypeError::BreakOutsideLoop {
                    keyword: if matches!(self.ast.stmts[id], Stmt::Break(_)) {
                        "break"
                    } else {
                        "continue"
                    },
                    span: self.ast.stmts.span(id).clone(),
                });
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            // `@if DEBUG { … }`: only the branch it picks is there, and checked.
            Stmt::If {
                condition,
                then,
                otherwise,
            } if self.ast.settled.contains(&id) => {
                let bool = self.u.table.bool;
                self.expect(*condition, bool, None);
                match self.settled_bool(*condition) {
                    Some(holds) => {
                        self.settled_ifs.insert(id, holds);
                        match (holds, otherwise) {
                            (true, _) => self.check_stmts(then),
                            (false, Some(otherwise)) => self.check_stmts(otherwise),
                            (false, None) => {}
                        }
                    }
                    None => self.errors.push(TypeError::NotSettled {
                        what: "an `@if`'s condition",
                        hint: "use literals and constants; a generic function's parameters \
                               can't be used in one yet",
                        span: self.span(*condition),
                    }),
                }
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let facts = self.check_condition(*condition);
                let (_, then_kept) = self.narrowed_during(facts.when_true, |c| c.check_stmts(then));
                let otherwise_kept = match otherwise {
                    Some(otherwise) => {
                        self.narrowed_during(facts.when_false, |c| c.check_stmts(otherwise))
                            .1
                    }
                    None => facts.when_false,
                };
                // A branch that leaves (`return`, `break`, `continue`) means the rest of the block
                // only runs after the other: `if v == none { return 0; }` narrows `v` from here
                // on, until the end of the block.
                let then_leaves = self.leaves(then);
                let otherwise_leaves = otherwise.as_ref().is_some_and(|o| self.leaves(o));
                if then_leaves && !otherwise_leaves {
                    self.narrowed.extend(otherwise_kept);
                } else if otherwise_leaves && !then_leaves {
                    self.narrowed.extend(then_kept);
                }
            }
        }
    }

    /// Reports statement `expr` if its value is thrown away and working it out does nothing
    /// else, so the statement can't be what was meant: `x == 5;`, `p.read(i);`, `a + b;`.
    /// A function's result isn't: calling it may be the point.
    fn check_unused(&mut self, expr: Id<Expr>) {
        let ty = self.exprs[expr.id].map(|t| self.u.shallow(t));
        if ty.is_none_or(|t| matches!(self.u.get(t), Ty::Unit | Ty::Error)) {
            return;
        }
        let span = self.span(expr);
        let kind = match &self.ast.exprs[expr] {
            Expr::Binary {
                op: BinOp::Equals, ..
            } => {
                let text = span.get_text().replacen("==", "=", 1);
                Unused::Comparison(text)
            }
            Expr::MethodCall { .. } => {
                let pure_pointer = self
                    .pointer_methods
                    .get(&expr)
                    .is_some_and(|m| !matches!(m, PointerMethod::Free | PointerMethod::Write));
                let pure_array = self
                    .array_methods
                    .get(&expr)
                    .is_some_and(|m| !matches!(m, ArrayMethod::Set));
                if !pure_pointer && !pure_array {
                    return;
                }
                Unused::Value
            }
            Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None
            | Expr::Var(_)
            | Expr::Field { .. }
            | Expr::Borrow { .. }
            | Expr::Deref(_)
            | Expr::Cast { .. }
            | Expr::Range { .. }
            | Expr::Array(_)
            | Expr::ArrayRepeat { .. }
            | Expr::Construct { .. }
            | Expr::Binary { .. }
            | Expr::Not(_)
            | Expr::Neg(_)
            | Expr::BitNot(_) => Unused::Value,
            _ => return,
        };
        self.errors.push(TypeError::UnusedValue { kind, span });
    }

    /// Whether control never gets past the end of `stmts`: the last one returns, breaks or
    /// continues; or is an `if` whose branches all do; or a `loop` with no `break` in it.
    fn leaves(&self, stmts: &[Id<Stmt>]) -> bool {
        let Some(&last) = stmts.last() else {
            return false;
        };
        match &self.ast.stmts[last] {
            Stmt::Return(_) | Stmt::Break(_) | Stmt::Continue(_) => true,
            // `@if`: the branch it picks.
            Stmt::If {
                then, otherwise, ..
            } if let Some(&holds) = self.settled_ifs.get(&last) => match holds {
                true => self.leaves(then),
                false => otherwise.as_ref().is_some_and(|o| self.leaves(o)),
            },
            Stmt::If {
                then,
                otherwise: Some(otherwise),
                ..
            } => self.leaves(then) && self.leaves(otherwise),
            Stmt::Loop { body } => !self.breaks(body),
            Stmt::Expr(expr) => self.expr_leaves(*expr),
            _ => false,
        }
    }

    /// Whether control never gets past `expr`: a block with no value that `leaves`, or a
    /// `match` all of whose arms do.
    fn expr_leaves(&self, expr: Id<Expr>) -> bool {
        match &self.ast.exprs[expr] {
            Expr::Unreachable | Expr::Fail(_) => true,
            Expr::Block { stmts, value: None } => self.leaves(stmts),
            Expr::Block {
                stmts,
                value: Some(value),
            } => self.leaves(stmts) || self.expr_leaves(*value),
            Expr::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|arm| self.expr_leaves(arm.body))
            }
            Expr::Conditional {
                then, otherwise, ..
            } => self.expr_leaves(*then) && self.expr_leaves(*otherwise),
            _ => false,
        }
    }

    /// Whether there's a `break` anywhere in `stmts`, which could leave a `loop` around them.
    fn breaks(&self, stmts: &[Id<Stmt>]) -> bool {
        stmts.iter().any(|&stmt| match &self.ast.stmts[stmt] {
            Stmt::Break(_) => true,
            Stmt::Loop { body } | Stmt::While { body, .. } | Stmt::For { body, .. } => {
                self.breaks(body)
            }
            Stmt::If {
                then, otherwise, ..
            } => self.breaks(then) || otherwise.as_ref().is_some_and(|o| self.breaks(o)),
            Stmt::Expr(expr) => self.expr_breaks(*expr),
            _ => false,
        })
    }

    /// Whether there's a `break` in a block or `match` arm `expr`.
    fn expr_breaks(&self, expr: Id<Expr>) -> bool {
        match &self.ast.exprs[expr] {
            Expr::Block { stmts, .. } => self.breaks(stmts),
            Expr::Match { arms, .. } => arms.iter().any(|arm| self.expr_breaks(arm.body)),
            Expr::Conditional {
                then, otherwise, ..
            } => self.expr_breaks(*then) || self.expr_breaks(*otherwise),
            _ => false,
        }
    }

    /// What a `for` loop over `iterable`, of type `ty`, binds each time round, recording how
    /// it goes through it.
    fn loop_element(&mut self, stmt: Id<Stmt>, iterable: Id<Expr>, ty: TypeId) -> TypeId {
        let error = self.u.table.error;
        // `for x in [1, 2, 3]`: a collection nothing else decides is an array.
        if let Some(VarKind::Collection { element, len }) = self.u.var_kind(ty) {
            let array = self.u.intern(Ty::Array { element, len });
            let _ = self.u.unify(ty, array);
        }
        let shallow = self.u.shallow(ty);
        // What's looped over is given up to the loop, or lent: `&value`, `&mut value`.
        let (viewed, lent) = match *self.u.get(shallow) {
            Ty::Ref { to, mutable } => (self.u.shallow(to), Some(mutable)),
            _ => (shallow, None),
        };
        // `for x in &[1, 2, 3]`: lent, it's an array too.
        if let Some(VarKind::Collection { element, len }) = self.u.var_kind(viewed) {
            let array = self.u.intern(Ty::Array { element, len });
            let _ = self.u.unify(viewed, array);
        }
        let viewed = self.u.shallow(viewed);
        let element = match *self.u.get(shallow) {
            Ty::Range(int) => {
                self.for_loops.insert(stmt, Loop::Range);
                return int;
            }
            Ty::Error => return error,
            _ => match self.u.get(viewed).clone() {
                // An array it owns, of values that move: given up, through its iterator.
                Ty::Array { element, .. } if lent.is_none() && !self.is_copy(element) => {
                    match self.iterator_loop(stmt, ty) {
                        Some(element) => return element,
                        None => element,
                    }
                }
                Ty::Array { element, .. } | Ty::Slice(element) => element,
                // Anything that's an `Iterator`, `IntoIterator`, `Iterable` or `IterableMut`:
                // what it gives.
                _ if let Some(element) = self.iterator_loop(stmt, ty) => return element,
                _ => {
                    let text = *self.u.get(viewed) == Ty::Str
                        || self.u.var_kind(ty) == Some(VarKind::Text);
                    self.errors.push(TypeError::NotIterable {
                        ty: self.describe(ty),
                        span: self.span(iterable),
                        text,
                    });
                    return error;
                }
            },
        };
        // Lent to change: references to change them. Otherwise copies of values that copy,
        // and references to those that move.
        let take = match lent {
            Some(true) => Take::RefMut,
            _ if self.is_copy(element) => Take::Copy,
            _ => Take::Ref,
        };
        self.for_loops.insert(stmt, Loop::Elements { take });
        match take {
            Take::Copy => element,
            Take::Ref | Take::RefMut => self.u.intern(Ty::Ref {
                mutable: take == Take::RefMut,
                to: element,
            }),
        }
    }

    /// Checks a condition: a `bool`, or an optional, which is true when it holds a value
    /// (`if x` is `if x != none`), combined with `&&`, `||` and `!`. Returns what it being
    /// true, or false, says about optional places.
    fn check_condition(&mut self, condition: Id<Expr>) -> Facts {
        let bool = self.u.table.bool;
        let facts = match &self.ast.exprs[condition] {
            // The right side only runs when the left is true: `x != none and x > 3`.
            Expr::Binary {
                op: BinOp::And,
                lhs,
                rhs,
            } => {
                let left = self.check_condition(*lhs);
                let (right, left_true) =
                    self.narrowed_during(left.when_true, |c| c.check_condition(*rhs));
                Facts {
                    when_true: left_true.into_iter().chain(right.when_true).collect(),
                    when_false: Vec::new(),
                }
            }
            // The right side only runs when the left is false: `x == none or x > 3`.
            Expr::Binary {
                op: BinOp::Or,
                lhs,
                rhs,
            } => {
                let left = self.check_condition(*lhs);
                let (right, left_false) =
                    self.narrowed_during(left.when_false, |c| c.check_condition(*rhs));
                Facts {
                    when_true: Vec::new(),
                    when_false: left_false.into_iter().chain(right.when_false).collect(),
                }
            }
            Expr::Not(inner) => {
                let inner = self.check_condition(*inner);
                Facts {
                    when_true: inner.when_false,
                    when_false: inner.when_true,
                }
            }
            _ => return self.check_condition_operand(condition),
        };
        self.exprs[condition.id] = Some(bool);
        facts
    }

    /// Checks a condition that isn't `&&`, `||` or `!`.
    fn check_condition_operand(&mut self, condition: Id<Expr>) -> Facts {
        let mut facts = Facts::default();
        match self.condition_narrowing(condition) {
            Some((narrowing, true)) => facts.when_true.push(narrowing),
            Some((narrowing, false)) => facts.when_false.push(narrowing),
            None => {}
        }
        facts
    }

    /// Checks a condition that isn't `&&`, `||` or `!`, returning the optional place it tells
    /// something about, and whether that holds a value when the condition is true.
    fn condition_narrowing(&mut self, condition: Id<Expr>) -> Option<(Narrowing, bool)> {
        let bool = self.u.table.bool;
        let ty = self.infer_optional(condition);
        let shallow = self.u.shallow(ty);
        if let Ty::Optional(inner) = *self.u.get(shallow) {
            if matches!(self.u.get(self.u.shallow(inner)), Ty::Bool) {
                self.errors.push(TypeError::OptionalBoolCondition {
                    span: self.span(condition),
                });
                return None;
            }
            return Some((self.narrowing(condition, inner)?, true));
        }
        if !self.coerce(condition, bool, ty) {
            self.mismatch(bool, ty, self.span(condition), None);
            return None;
        }
        // `x != none`, `x == none`, `x == 5` and `x != 5`.
        let Expr::Binary { op, lhs, rhs } = &self.ast.exprs[condition] else {
            return None;
        };
        let equals = match op {
            BinOp::Equals => true,
            BinOp::NotEquals => false,
            _ => return None,
        };
        let against_none = matches!(self.ast.exprs[*lhs], Expr::None)
            || matches!(self.ast.exprs[*rhs], Expr::None);
        // Equal to `none` means it holds nothing; equal to a value means it holds one.
        let holds_value = equals != against_none;
        let is_optional = |c: &Self, e: Id<Expr>| {
            c.exprs[e.id].is_some_and(|ty| matches!(c.u.get(c.u.shallow(ty)), Ty::Optional(_)))
        };
        let optional = match (&self.ast.exprs[*lhs], &self.ast.exprs[*rhs]) {
            (_, Expr::None) => *lhs,
            (Expr::None, _) => *rhs,
            // `x == 5`: only equal if `x` holds a value. Not for two optionals: both could be
            // `none`.
            _ => match (is_optional(self, *lhs), is_optional(self, *rhs)) {
                (true, false) => *lhs,
                (false, true) => *rhs,
                _ => return None,
            },
        };
        let ty = self.exprs[optional.id]?;
        let Ty::Optional(inner) = *self.u.get(self.u.shallow(ty)) else {
            return None;
        };
        Some((self.narrowing(optional, inner)?, holds_value))
    }

    /// A narrowing of `expr` to the `inner` type it holds, if it's a place that can be narrowed.
    fn narrowing(&mut self, expr: Id<Expr>, inner: TypeId) -> Option<Narrowing> {
        let key = self.key(expr)?;
        self.next_narrowing += 1;
        Some(Narrowing {
            id: self.next_narrowing,
            key,
            inner,
        })
    }

    /// After an optional place of type `ty` is assigned `value`: if that was a plain value,
    /// wrapped, the place holds it until the end of the block.
    fn narrow_if_assigned_a_value(&mut self, key: Option<Key>, ty: TypeId, value: Id<Expr>) {
        let Some(key) = key else { return };
        if !self.wrapped.contains(&value) {
            return;
        }
        let Ty::Optional(inner) = *self.u.get(self.u.shallow(ty)) else {
            return;
        };
        self.next_narrowing += 1;
        self.narrowed.push(Narrowing {
            id: self.next_narrowing,
            key,
            inner,
        });
    }

    /// Runs `f` with `narrowings` in effect, returning its result and the narrowings it didn't
    /// end.
    fn narrowed_during<T>(
        &mut self,
        narrowings: Vec<Narrowing>,
        f: impl FnOnce(&mut Self) -> T,
    ) -> (T, Vec<Narrowing>) {
        let ids: HashSet<usize> = narrowings.iter().map(|n| n.id).collect();
        self.narrowed.extend(narrowings);
        let result = f(self);
        // They may have ended early; others ended inside stay ended.
        let (still, others): (Vec<Narrowing>, Vec<Narrowing>) = std::mem::take(&mut self.narrowed)
            .into_iter()
            .partition(|n| ids.contains(&n.id));
        self.narrowed = others;
        (result, still)
    }

    // --- Expressions ---

    /// Infers the type of `expr` and requires it to be `expected`.
    fn expect(&mut self, expr: Id<Expr>, expected: TypeId, expected_from: Option<Span>) {
        self.expected = Some(expected);
        let found = self.infer(expr);
        if self.coerce(expr, expected, found) {
            return;
        }
        // A `string` where C needs a `cstr`: say why they differ.
        if self.u.shallow(expected) == self.u.table.cstr
            && self.u.shallow(found) == self.u.table.text
        {
            self.errors.push(TypeError::StrToC {
                span: self.span(expr),
            });
            return;
        }
        // `println(xs.get(0))` of a list nothing's said the values of yet: which `From`
        // converts it is chosen once it has, after the bodies.
        if self.conversion_undecided(expected, found) {
            self.deferred_conversions
                .push((expr, expected, found, expected_from));
            return;
        }
        self.mismatch_at(expr, expected, found, expected_from);
    }

    /// Makes `expr`, of type `found`, usable where `expected` is required, returning whether
    /// it is. A `T` is also usable as a `T?`: it's wrapped.
    fn coerce(&mut self, expr: Id<Expr>, expected: TypeId, found: TypeId) -> bool {
        // `I::Item` of an `I` that's known by now is what its implementation says.
        let (expected, found) = (self.normalize_assoc(expected), self.normalize_assoc(found));
        if self.u.unify(expected, found).is_ok() {
            return true;
        }
        let expected = self.u.shallow(expected);
        // A `&mut T` where a `&T` is expected: lent as `&*r`, which leaves `r` to use after.
        if let Ty::Ref {
            mutable: false,
            to: wanted,
        } = *self.u.get(expected)
            && let Ty::Ref {
                mutable: true,
                to: given,
            } = *self.u.get(self.u.shallow(found))
            && self.u.unify(wanted, given).is_ok()
        {
            self.downgraded.insert(expr);
            return true;
        }
        if self.slices_to(expected, found) {
            self.sliced.insert(expr, expected);
            return true;
        }
        if let Ty::Optional(inner) = *self.u.get(expected) {
            if self.u.unify(inner, found).is_ok() {
                self.wrapped.insert(expr);
                return true;
            }
            if self.slices_to(inner, found) {
                self.sliced.insert(expr, inner);
                self.wrapped.insert(expr);
                return true;
            }
        }
        if self.from_conversion(expr, expected, found) {
            return true;
        }
        // `&x` of a narrowed `x` borrows the value, but where a reference to the optional is
        // expected, it borrows the optional: `read(&o)` for `fn read(o: &(i32?))`.
        if let Expr::Borrow {
            mutable,
            expr: borrowed,
        } = self.ast.exprs[expr]
            && self.narrowed_reads.contains(&borrowed)
        {
            let value = self.exprs[borrowed.id].unwrap_or(self.u.table.error);
            let optional = self.u.intern(Ty::Optional(value));
            let reference = self.u.intern(Ty::Ref {
                mutable,
                to: optional,
            });
            if self.u.unify(expected, reference).is_ok() {
                self.narrowed_reads.remove(&borrowed);
                self.exprs[borrowed.id] = Some(optional);
                self.exprs[expr.id] = Some(reference);
                // Through `&mut` of the optional, it can be made `none`.
                if mutable && let Some(borrowed) = self.key(borrowed) {
                    self.forget(|key| key.within(&borrowed));
                }
                return true;
            }
        }
        false
    }

    /// Whether a value of type `found` becomes the slice `expected`: a reference to an array of
    /// the same elements (`&numbers` for a `[i32]`, `&mut numbers` for a `[mut i32]`), or a
    /// `[mut T]` where a `[T]` is.
    fn slices_to(&mut self, expected: TypeId, found: TypeId) -> bool {
        let shallow = |c: &Self, ty: TypeId| c.u.get(c.u.shallow(ty)).clone();
        let Ty::Ref {
            mutable: wants_mut,
            to: slice,
        } = shallow(self, expected)
        else {
            return false;
        };
        let Ty::Slice(element) = shallow(self, slice) else {
            return false;
        };
        let Ty::Ref {
            mutable: is_mut,
            to: viewed,
        } = shallow(self, found)
        else {
            return false;
        };
        // A `[mut T]` needs something it may change.
        if wants_mut && !is_mut {
            return false;
        }
        // `&b` of a collection nothing has decided yet: borrowing it makes it the array.
        if let Some(VarKind::Collection { element, len }) = self.u.var_kind(viewed) {
            let array = self.u.intern(Ty::Array { element, len });
            let _ = self.u.unify(viewed, array);
        }
        match shallow(self, viewed) {
            Ty::Array { element: found, .. } => self.u.unify(element, found).is_ok(),
            Ty::Slice(found) => is_mut && !wants_mut && self.u.unify(element, found).is_ok(),
            _ => false,
        }
    }

    /// What unwrapping an optional of type `ty` gives: the `T` a `T?` holds, or a reference
    /// to it through a reference to the optional (`&T?` gives `&T`), which leaves the
    /// optional where it is. Reports an error if `ty` isn't either.
    fn optional_inner(&mut self, ty: TypeId, expr: Id<Expr>) -> TypeId {
        let shallow = self.u.shallow(ty);
        if let Ty::Ref { mutable, to } = *self.u.get(shallow)
            && let Ty::Optional(inner) = *self.u.get(self.u.shallow(to))
        {
            return self.u.intern(Ty::Ref { mutable, to: inner });
        }
        let inner = self.u.fresh(VarKind::Any);
        let optional = self.u.intern(Ty::Optional(inner));
        if self.u.unify(optional, ty).is_err() {
            self.errors.push(TypeError::NotOptional {
                ty: self.describe(ty),
                span: self.span(expr),
            });
            return self.u.table.error;
        }
        inner
    }

    fn infer(&mut self, id: Id<Expr>) -> TypeId {
        let in_place = std::mem::replace(&mut self.in_place, false);
        self.expected_here = self.expected.take();
        let mut ty = self.infer_inner(id, in_place);
        // `I::Item` of an `I` the expression's arguments decided: what its implementation says.
        ty = self.normalize_assoc(ty);
        if let Some(inner) = self.narrowed_type(id) {
            self.narrowed_reads.insert(id);
            ty = inner;
        }
        self.exprs[id.id] = Some(ty);
        ty
    }

    /// For `==` with an optional on either side, the types whose values are compared: what each
    /// optional holds. `None` if neither side is optional.
    fn optional_operands(&self, lhs: TypeId, rhs: TypeId) -> Option<(TypeId, TypeId)> {
        let inner = |ty: TypeId| match *self.u.get(self.u.shallow(ty)) {
            Ty::Optional(inner) => Some(inner),
            _ => None,
        };
        match (inner(lhs), inner(rhs)) {
            (None, None) => None,
            (l, r) => Some((l.unwrap_or(lhs), r.unwrap_or(rhs))),
        }
    }

    /// Infers `expr` as the optional it is, even where it's narrowed: for `if x`, `x != none`
    /// and `x else …`, which ask about the optional itself.
    fn infer_optional(&mut self, expr: Id<Expr>) -> TypeId {
        let Some(key) = self.key(expr) else {
            return self.infer(expr);
        };
        let (lifted, kept): (Vec<Narrowing>, Vec<Narrowing>) = std::mem::take(&mut self.narrowed)
            .into_iter()
            .partition(|n| n.key.same(&key));
        self.narrowed = kept;
        let ty = self.infer(expr);
        self.narrowed.extend(lifted);
        ty
    }

    /// Infers `object`, whose place is part of a larger one: the `p` in `p.x` or `*p`.
    fn infer_object(&mut self, object: Id<Expr>) -> TypeId {
        self.in_place = true;
        self.infer(object)
    }

    /// `in_place`: `id` is the object of a field access or dereference, not a value on its own.
    fn infer_inner(&mut self, id: Id<Expr>, in_place: bool) -> TypeId {
        let table = &self.u.table;
        let (error, unit, bool) = (table.error, table.unit, table.bool);

        match &self.ast.exprs[id] {
            Expr::Hole => error,
            // `@const f(x)`: its value, of whatever's expected here. It names no locals, so
            // it's checked here like anything else.
            Expr::InPlace(c) => {
                self.checked_consts.insert(*c);
                self.expected = self.expected_here.take();
                self.infer(self.ast.consts[*c].value)
            }
            // Never gets a value: it's whatever's expected here, or if nothing decides,
            // `()` once checking is done.
            Expr::Unreachable => {
                let ty = (self.expected_here.take()).unwrap_or_else(|| self.u.fresh(VarKind::Any));
                self.unreachables.push(ty);
                ty
            }
            // `@fail("…")`: like `@unreachable`, of whatever's expected; its message is text.
            Expr::Fail(message) => {
                let ty = (self.expected_here.take()).unwrap_or_else(|| self.u.fresh(VarKind::Any));
                let text = self.u.table.text;
                self.expect(*message, text, None);
                self.unreachables.push(ty);
                ty
            }
            Expr::Int(_) => self.u.fresh(VarKind::Int),
            Expr::Float(_) => self.u.fresh(VarKind::Float),
            Expr::Bool(_) => bool,
            // A `str`, or a `cstr` where C needs one.
            Expr::Str(_) => self.u.fresh(VarKind::Text),
            // `"total: {n}"`: a `string`, of the pieces and the values written in.
            Expr::Interpolated(parts) => {
                for &part in parts {
                    match part {
                        som_ast::Part::Text(text) => {
                            let ty = self.infer(text);
                            self.text_as_string(ty);
                        }
                        som_ast::Part::Value(value) => {
                            let ty = self.infer(value);
                            self.text_as_string(ty);
                            self.displayed.push((value, ty));
                        }
                    }
                }
                self.u.table.text
            }
            Expr::None => {
                let inner = self.u.fresh(VarKind::Any);
                self.u.intern(Ty::Optional(inner))
            }
            // A collection: an array, or a slice where one is expected.
            Expr::Array(items) => {
                let element = self.u.fresh(VarKind::Any);
                let found: Vec<TypeId> = items.iter().map(|&item| self.infer(item)).collect();
                // `[1, none, 5]`: with an optional among them, they're all optionals, and the
                // plain values are wrapped, as in `x if c else none`.
                if let Some(&optional) = found
                    .iter()
                    .find(|&&ty| matches!(self.u.get(self.u.shallow(ty)), Ty::Optional(_)))
                {
                    let _ = self.u.unify(element, optional);
                }
                for (&item, &ty) in items.iter().zip(&found) {
                    if !self.coerce(item, element, ty) {
                        self.mismatch(element, ty, self.span(item), None);
                    }
                }
                let len = self.u.intern(Ty::Value(Value::Int(items.len() as u128)));
                self.u.fresh(VarKind::Collection { element, len })
            }
            Expr::Cast { value, ty } => {
                let target = self.lower(*ty);
                let found = self.infer(*value);
                // A literal converted to an integer type is read in full, as an `i128`, then
                // wrapped: `18446744073709551617 as u64` is 1, `-1 as u64` the largest.
                if matches!(self.ast.exprs[*value], Expr::Int(_))
                    && self.u.var_kind(found) == Some(VarKind::Int)
                    && matches!(self.u.get(self.u.shallow(target)), Ty::Int(_))
                {
                    let i128 = self.u.intern(Ty::Int(IntTy::I128));
                    let _ = self.u.unify(found, i128);
                }
                // The value converts at run time: `300 as u8` is the `i32` 300, wrapped to 44.
                let number = |c: &Self, ty: TypeId| {
                    matches!(
                        c.u.get(c.u.shallow(ty)),
                        Ty::Int(_) | Ty::Float(_) | Ty::Error
                    ) || matches!(c.u.var_kind(ty), Some(VarKind::Int | VarKind::Float))
                };
                // An enum whose variants hold nothing converts to the number of its variant.
                let plain_enum = self.is_plain_enum(self.u.get(self.u.shallow(found)))
                    && matches!(self.u.get(self.u.shallow(target)), Ty::Int(_));
                // A raw pointer to another, `null() as *T`; or a reference to one, `&items as *T`.
                let pointer = |c: &Self, ty: TypeId| {
                    matches!(c.u.get(c.u.shallow(ty)), Ty::Pointer | Ty::Ptr(_))
                };
                let reference =
                    |c: &Self, ty: TypeId| matches!(c.u.get(c.u.shallow(ty)), Ty::Ref { .. });
                let pointers =
                    (pointer(self, found) || reference(self, found)) && pointer(self, target);
                // C's text is a pointer to its bytes: `bytes as cstr`, `text as *u8`.
                let cstr = |c: &Self, ty: TypeId| matches!(c.u.get(c.u.shallow(ty)), Ty::CStr);
                let c_text = (cstr(self, found) && pointer(self, target))
                    || (pointer(self, found) && cstr(self, target));
                if !plain_enum
                    && !pointers
                    && !c_text
                    && (!number(self, found) || !number(self, target))
                {
                    self.errors.push(TypeError::InvalidCast {
                        from: self.describe(found),
                        to: self.describe(target),
                        span: self.span(id),
                    });
                }
                target
            }
            Expr::Range { start, end } => {
                let int = self.u.fresh(VarKind::Int);
                for &bound in start.iter().chain(end) {
                    self.expect(bound, int, None);
                }
                // Only `slice` knows where an open range ends; it checks those itself.
                if start.is_none() || end.is_none() {
                    self.errors.push(TypeError::OpenRange {
                        span: self.span(id),
                    });
                }
                self.u.intern(Ty::Range(int))
            }
            Expr::ArrayRepeat { value, count } => {
                let element = self.infer(*value);
                if !self.is_copy(element) {
                    self.errors.push(TypeError::NotCopyable {
                        ty: self.describe(element),
                        span: self.span(*value),
                        repeat: true,
                    });
                }
                let len = self.lower_value(*count, None);
                self.counts.insert(id, len);
                self.u.fresh(VarKind::Collection { element, len })
            }
            Expr::Fallback { optional, fallback } => {
                // What's expected of the whole, `&mut Pair`, is expected of the optional
                // wrapped: so `get(i) else …` picks the `get` giving a `&mut Pair?`.
                if let Some(expected) = self.expected_here.take() {
                    self.expected = Some(self.u.intern(Ty::Optional(expected)));
                }
                let ty = self.infer_optional(*optional);
                let inner = self.optional_inner(ty, *optional);
                let from = self.span(*optional);
                self.expect(*fallback, inner, Some(from));
                inner
            }

            // `usize::MAX`: of the type it's the limit of.
            Expr::Var(path) if let Some((primitive, _)) = path.int_limit() => {
                self.u.intern(Ty::from_primitive(primitive))
            }
            Expr::Var(_) => match self
                .names
                .expr(id)
                .map(|def| (def, &self.names.def(def).kind))
            {
                // An `out` parameter is a pointer, but it's used as what it points to.
                Some((def, &DefKind::Param { func, index }))
                    if self.ast.funcs[func].params[index].out =>
                {
                    let ty = self.locals.get(&def).copied().unwrap_or(error);
                    match self.u.get(ty) {
                        Ty::Ref { to, .. } => *to,
                        _ => error,
                    }
                }
                Some((
                    def,
                    DefKind::Param { .. }
                    | DefKind::Local(_)
                    | DefKind::Out(_)
                    | DefKind::Binding(_),
                )) => {
                    let ty = self.locals.get(&def).copied().unwrap_or(error);
                    // Passed on, a `&mut` can change what's behind it.
                    if !in_place && self.is_mut_ref(ty) {
                        self.forget(|key| key.root == def && !key.path.is_empty());
                    }
                    ty
                }
                // `Shape::Empty`
                Some((def, DefKind::Variant { .. })) => {
                    let (ty, s, index, _) = self.variant_value(def);
                    self.fit_alias(id, ty);
                    let kind = self.ast.structs[s].variants()[index].kind;
                    if kind != VariantKind::Unit {
                        self.errors.push(TypeError::WrongVariantForm {
                            name: self.variant_name(s, index),
                            holds: holds(kind),
                            form: self.variant_form(s, index),
                            span: self.span(id),
                        });
                    }
                    ty
                }
                // `W` for `@const W = 60;`
                Some((_, &DefKind::Const(c))) => self.check_const(c),
                // `N` in `struct Buffer<N: usize>`: the number it is in each copy.
                Some((def, _)) if let Some(&ty) = self.value_params.get(&def) => ty,
                Some((def, DefKind::Func(func))) => {
                    let expected = self.expected_here.take();
                    let func = self.pick_function(id, def, expected).unwrap_or(*func);
                    let (sig, args) = self.instantiate(func);
                    self.require_bounds(func, &args, self.span(id));
                    if !args.is_empty() {
                        self.type_args.insert(id, args);
                    }
                    self.u.intern(Ty::Fn {
                        params: sig.params,
                        ret: sig.ret,
                    })
                }
                _ => error,
            },

            Expr::Binary {
                op: BinOp::And | BinOp::Or,
                ..
            } => {
                self.check_condition(id);
                bool
            }

            Expr::Binary { op, lhs, rhs } => {
                // `x == none` and `x != none` ask whether `x` holds a value.
                let against_none = matches!(op, BinOp::Equals | BinOp::NotEquals)
                    && (matches!(self.ast.exprs[*lhs], Expr::None)
                        || matches!(self.ast.exprs[*rhs], Expr::None));
                let (lhs_ty, rhs_ty) = if against_none {
                    (self.infer_optional(*lhs), self.infer_optional(*rhs))
                } else {
                    (self.infer(*lhs), self.infer(*rhs))
                };
                // `name + "!"`: text in quotes is a `string`, with its operators.
                self.text_as_string(lhs_ty);
                self.text_as_string(rhs_ty);
                if against_none {
                    if self.u.unify(lhs_ty, rhs_ty).is_err() {
                        let ((optional, ty), none) = if matches!(self.ast.exprs[*lhs], Expr::None) {
                            ((*rhs, rhs_ty), *lhs)
                        } else {
                            ((*lhs, lhs_ty), *rhs)
                        };
                        self.errors.push(TypeError::NotOptional {
                            ty: self.describe(ty),
                            span: self.span(optional),
                        });
                        // Already reported: don't also say the `none` can't be inferred.
                        self.exprs[none.id] = Some(error);
                    }
                    return bool;
                }
                // `x == 5` on an optional: true only if it holds 5.
                if matches!(op, BinOp::Equals | BinOp::NotEquals)
                    && let Some((lhs_inner, rhs_inner)) = self.optional_operands(lhs_ty, rhs_ty)
                {
                    // The values are compared, but errors name the types as written.
                    let comparable = self.u.unify(lhs_inner, rhs_inner).is_ok();
                    match self.operand_ok(*op, lhs_inner) {
                        Some(true) if comparable => {}
                        None if comparable => {
                            self.deferred_ops.push((id, *op, lhs_inner, rhs_inner))
                        }
                        _ => self.invalid_operands(id, *op, lhs_ty, rhs_ty),
                    }
                    return bool;
                }
                let lhs_ty = self.through_reference(*lhs, lhs_ty);
                let rhs_ty = self.through_reference(*rhs, rhs_ty);
                self.binary(id, *op, lhs_ty, rhs_ty)
            }

            // `!x` of an optional is `x == none`.
            Expr::Not(_) => {
                self.check_condition(id);
                bool
            }

            // `~x`: an integer, the same type back.
            Expr::BitNot(inner) => {
                let ty = self.infer(*inner);
                let shallow = self.u.shallow(ty);
                let integer = matches!(self.u.get(shallow), Ty::Int(_) | Ty::Error)
                    || self.u.var_kind(shallow) == Some(VarKind::Int);
                if !integer {
                    self.errors.push(TypeError::CannotFlipBits {
                        ty: self.describe(ty),
                        span: self.span(id),
                    });
                    return self.u.table.error;
                }
                ty
            }

            // `-x`: a signed integer or a float, the same type back.
            Expr::Neg(inner) => {
                let ty = self.infer(*inner);
                let shallow = self.u.shallow(ty);
                let negatable = match (self.u.get(shallow), self.u.var_kind(shallow)) {
                    (Ty::Int(int), _) => int.is_signed(),
                    (Ty::Float(_) | Ty::Error, _) => true,
                    (_, Some(VarKind::Int | VarKind::Float)) => true,
                    _ => false,
                };
                if !negatable {
                    self.errors.push(TypeError::CannotNegate {
                        ty: self.describe(ty),
                        span: self.span(id),
                    });
                    return self.u.table.error;
                }
                ty
            }

            Expr::Borrow { mutable, expr } => {
                // `&mut p` can change anything inside `p`, other than through what's narrowed:
                // `&mut x` of a narrowed `x` borrows the value it holds.
                if *mutable && let Some(borrowed) = self.key(*expr) {
                    self.forget(|key| key.within(&borrowed) && !key.same(&borrowed));
                }
                let to = self.infer(*expr);
                if *mutable {
                    self.check_mutable(*expr, true);
                }
                self.u.intern(Ty::Ref {
                    mutable: *mutable,
                    to,
                })
            }

            // The parameter it's passed to says what the variable is.
            Expr::Out(_) => {
                let to = self.u.fresh(VarKind::Any);
                if let Some(def) = self.names.expr(id) {
                    self.locals.insert(def, to);
                }
                self.u.intern(Ty::Ref { mutable: true, to })
            }

            Expr::Deref(inner) => {
                let ty = self.infer_object(*inner);
                let ty = self.u.shallow(ty);
                match self.u.get(ty).clone() {
                    Ty::Ref { to, .. }
                        if matches!(self.u.get(self.u.shallow(to)), Ty::Slice(_) | Ty::Str) =>
                    {
                        self.errors.push(TypeError::DerefSlice {
                            span: self.span(id),
                        });
                        error
                    }
                    Ty::Ref { to, .. } => to,
                    Ty::Error => error,
                    Ty::Var(_) if self.u.var_kind(ty) == Some(VarKind::Any) => {
                        self.errors.push(TypeError::CannotInfer {
                            span: self.span(*inner),
                        });
                        error
                    }
                    _ => {
                        self.errors.push(TypeError::CannotDeref {
                            ty: self.describe(ty),
                            span: self.span(*inner),
                        });
                        error
                    }
                }
            }

            Expr::Assign { target, value } => {
                // The target is the optional itself, not the value it held: `x = none` is fine.
                let assigned = self.key(*target);
                let (lifted, kept) = std::mem::take(&mut self.narrowed)
                    .into_iter()
                    .partition(|n| assigned.as_ref().is_some_and(|a| n.key.same(a)));
                self.narrowed = kept;
                let outer = std::mem::replace(&mut self.assigning, true);
                let target_ty = self.infer(*target);
                self.assigning = outer;
                // After inferring it: whether `p.x` is a field or a property depends on `p`.
                if let Expr::Field { object, field } = &self.ast.exprs[*target]
                    && self.is_prop(*target)
                {
                    let object_ty = self.exprs[object.id].unwrap_or(self.u.table.error);
                    self.errors.push(TypeError::AssignToProp {
                        name: field.name.to_string(),
                        span: self.span(*target),
                        field: self.hidden_field_named(object_ty, field),
                    });
                } else if !self.is_place(*target) {
                    self.errors.push(TypeError::InvalidAssignTarget {
                        span: self.span(*target),
                    });
                }
                self.check_mutable(*target, false);
                // The value is worked out first, while the target still holds what it did:
                // `x = x + 1` reads the narrowed `x`.
                self.narrowed.extend::<Vec<Narrowing>>(lifted);
                let from = self.span(*target);
                self.expect(*value, target_ty, Some(from));
                // Afterwards it holds what was assigned.
                if let Some(assigned) = &assigned {
                    self.forget(|key| key.within(assigned));
                }
                self.narrow_if_assigned_a_value(assigned, target_ty, *value);
                unit
            }

            Expr::Call {
                callee,
                owner_args,
                type_args: written,
                args,
            } => {
                let expected = self.expected_here.take();
                if let Some(def) = self.names.expr(id)
                    && self.names.overloads(def).len() > 1
                {
                    return self
                        .overloaded_call(id, callee, def, owner_args, written, args, expected);
                }
                match self.names.expr(id).map(|def| &self.names.def(def).kind) {
                    Some(DefKind::Func(func)) => {
                        let func = *func;
                        let (sig, type_args) = self.instantiate(func);
                        if !written.is_empty() {
                            self.written_type_args(id, func, written, &type_args);
                        }
                        // `Vector3::new(…)`: a method of the type the alias stands for.
                        if let Some(owner) = self.owner_type(func, &type_args) {
                            self.fit_alias(id, owner);
                        }
                        if !owner_args.is_empty() {
                            self.fit_owner_args(id, callee, func, &type_args, owner_args);
                        }
                        self.require_bounds(func, &type_args, self.span(id));
                        if !type_args.is_empty() {
                            self.type_args.insert(id, type_args);
                        }
                        self.check_args(id, &callee.to_string(), func, &sig.params, args);
                        sig.ret
                    }
                    // `Shape::Circle(1.0)`
                    Some(DefKind::Variant { .. }) => {
                        let def = self.names.expr(id).expect("matched above");
                        let (ty, s, index, fields) = self.variant_value(def);
                        self.fit_alias(id, ty);
                        let variant = &self.ast.structs[s].variants()[index];
                        if variant.kind != VariantKind::Tuple {
                            self.errors.push(TypeError::WrongVariantForm {
                                name: self.variant_name(s, index),
                                holds: holds(variant.kind),
                                form: self.variant_form(s, index),
                                span: self.span(id),
                            });
                            args.iter().for_each(|&a| {
                                self.infer(a);
                            });
                        } else if args.len() != fields.len() {
                            self.errors.push(TypeError::WrongArgCount {
                                name: callee.to_string(),
                                expected: fields.len(),
                                at_least: false,
                                found: args.len(),
                                span: self.span(id),
                            });
                            args.iter().for_each(|&a| {
                                self.infer(a);
                            });
                        } else {
                            for ((&arg, &field), declared) in
                                args.iter().zip(&fields).zip(&variant.fields)
                            {
                                let from = self.ast.types.span(declared.ty).clone();
                                self.expect(arg, field, Some(from));
                            }
                        }
                        ty
                    }
                    _ => {
                        args.iter().for_each(|&a| {
                            self.infer(a);
                        });
                        error
                    }
                }
            }

            Expr::MethodCall {
                receiver,
                method,
                args,
            } => {
                let expected = self.expected_here.take();
                let receiver_ty = self.infer(*receiver);
                // `node.total()` of a `#Node`: what it shares.
                let receiver_ty = self.through_shared(*receiver, receiver_ty, &method.name);
                // `"abc".len()`: text in quotes is a `string`, with its methods.
                self.text_as_string(receiver_ty);
                // A trait's method the type has, `x.compare(&y)`, before a pointer's own; an
                // array's or slice's own first, `get` giving a copy.
                let builtin = ArrayMethod::from_name(&method.name).is_some() && {
                    let mut ty = self.u.shallow(receiver_ty);
                    if let Ty::Ref { to, .. } = *self.u.get(ty) {
                        ty = self.u.shallow(to);
                    }
                    matches!(self.u.get(ty), Ty::Array { .. } | Ty::Slice(_) | Ty::Str)
                };
                if !builtin
                    && let Some(ty) =
                        self.trait_method_call(id, *receiver, receiver_ty, method, args)
                {
                    return ty;
                }
                // A built-in type's own method, from `impl i32 { … }`.
                let primitive = self
                    .primitive_owner(receiver_ty)
                    .filter(|&owner| self.names.method(owner, &method.name).is_some());
                if primitive.is_none() {
                    if let Some(ty) =
                        self.array_method(id, *receiver, receiver_ty, method, args, false)
                    {
                        return ty;
                    }
                    if let Some(ty) = self.pointer_method(id, receiver_ty, method, args) {
                        return ty;
                    }
                }
                // Text written in quotes is a `string` here.
                if primitive.is_some() && self.u.var_kind(receiver_ty) == Some(VarKind::Text) {
                    let _ = self.u.unify(receiver_ty, self.u.table.text);
                }
                // Any type can have methods now: one that isn't a struct just lacks this one.
                let viewed = {
                    let shallow = self.u.shallow(receiver_ty);
                    match self.u.get(shallow) {
                        Ty::Ref { to, .. } => self.u.shallow(*to),
                        _ => shallow,
                    }
                };
                let lacks = !matches!(self.u.get(viewed), Ty::Struct(..) | Ty::Error)
                    && !matches!(self.u.var_kind(viewed), Some(VarKind::Any));
                // `find(2).get()`: an optional may hold nothing, which is what to say.
                if primitive.is_none() && matches!(self.u.get(viewed), Ty::Optional(_)) {
                    let span = self.span(*receiver);
                    self.errors.push(TypeError::MemberOfOptional {
                        object: span.get_text().to_string(),
                        ty: self.describe(receiver_ty),
                        variable: matches!(self.ast.exprs[*receiver], Expr::Var(_)),
                        span,
                    });
                    args.iter().for_each(|&a| {
                        self.infer(a);
                    });
                    return error;
                }
                if primitive.is_none() && lacks {
                    self.errors.push(TypeError::UnknownMethod {
                        ty: self.describe(receiver_ty),
                        method: method.name.to_string(),
                        span: method.span.clone(),
                        methods: Vec::new(),
                        props: Vec::new(),
                        fields: Vec::new(),
                        bare_call: self.bare_call(id, method, args),
                    });
                    args.iter().for_each(|&a| {
                        self.infer(a);
                    });
                    return error;
                }
                let owner = match primitive {
                    Some(owner) => owner,
                    None => match self.struct_of(receiver_ty, *receiver) {
                        Some((owner, _)) => owner,
                        None => {
                            args.iter().for_each(|&a| {
                                self.infer(a);
                            });
                            return error;
                        }
                    },
                };
                let Some(def) = self.names.method(owner, &method.name) else {
                    let ty = self.describe(receiver_ty);
                    if !self.names.props(owner, &method.name).is_empty() {
                        let span = self.span(id);
                        self.errors.push(TypeError::MethodIsProp {
                            ty,
                            name: method.name.to_string(),
                            span: method.span.clone(),
                            parens: args
                                .is_empty()
                                .then(|| method.span.after().merge(&span.after())),
                        });
                        args.iter().for_each(|&a| {
                            self.infer(a);
                        });
                        return error;
                    }
                    let (fields, props, methods) = self.members(owner, receiver_ty, &method.span);
                    self.errors.push(TypeError::UnknownMethod {
                        ty,
                        method: method.name.to_string(),
                        span: method.span.clone(),
                        methods,
                        props,
                        fields,
                        bare_call: self.bare_call(id, method, args),
                    });
                    args.iter().for_each(|&a| {
                        self.infer(a);
                    });
                    return error;
                };
                if self.names.overloads(def).len() > 1 {
                    let name = format!("{}::{}", self.names.def(owner).name, method.name);
                    return self.overloaded_method(
                        id,
                        &name,
                        def,
                        *receiver,
                        receiver_ty,
                        args,
                        expected,
                    );
                }
                let DefKind::Func(func) = self.names.def(def).kind else {
                    unreachable!("methods are functions")
                };
                let (sig, type_args) = self.instantiate(func);
                if !self.on_impl_target(func, &type_args, receiver_ty, method, "method") {
                    args.iter().for_each(|&a| {
                        self.infer(a);
                    });
                    return error;
                }
                self.methods.insert(id, def);
                self.check_visible(def, &method.span);
                self.require_bounds(func, &type_args, self.span(id));
                if !type_args.is_empty() {
                    self.type_args.insert(id, type_args);
                }
                // The receiver is the first (`self`) parameter.
                match sig.params.split_first() {
                    Some((&self_ty, rest)) => {
                        if !self.receive(*receiver, self_ty, receiver_ty) {
                            let from = self.ast.funcs[func].params[0]
                                .ty
                                .map(|t| self.ast.types.span(t).clone());
                            self.mismatch(self_ty, receiver_ty, self.span(*receiver), from);
                        }
                        let name = format!("{}::{}", self.names.def(owner).name, method.name);
                        self.check_args_from(id, &name, func, 1, rest, args);
                    }
                    None => {
                        let name = format!("{}::{}", self.names.def(owner).name, method.name);
                        self.errors.push(TypeError::WrongArgCount {
                            name,
                            expected: 0,
                            at_least: false,
                            found: args.len() + 1,
                            span: self.span(id),
                        });
                    }
                }
                sig.ret
            }

            Expr::Field { object, field } => {
                let object_ty = self.infer_object(*object);
                // `node.value` of a `#Node`: what it shares.
                let object_ty = self.through_shared(*object, object_ty, &field.name);
                // `"abc".len`: text in quotes is a `string`, with its props.
                self.text_as_string(object_ty);
                // `list.len`, with a private field and a `pub prop` both called `len`: the
                // field where it's seen, so its own code can change it, the prop elsewhere.
                if !self.field_seen(object_ty, field)
                    && let Some(ty) = self.prop_access(id, *object, object_ty, field)
                {
                    return ty;
                }
                // `items.len`: the length of an array, slice or text.
                if &*field.name == "len"
                    && let Some(ty) = self.array_method(id, *object, object_ty, field, &[], true)
                {
                    return ty;
                }
                // `xs.length`: an array or slice has `len` and its methods, nothing else.
                if self.is_collection(object_ty) {
                    let methods = (ArrayMethod::ALL.iter())
                        .map(|&(name, _)| name)
                        .filter(|&name| name != "len" && name != "bytes")
                        .map(String::from)
                        .collect();
                    self.errors.push(TypeError::UnknownField {
                        ty: self.u.display(self.u.shallow(object_ty), self.names),
                        field: field.name.to_string(),
                        available: Vec::new(),
                        props: vec!["len".into()],
                        methods,
                        span: field.span.clone(),
                    });
                    return error;
                }
                // A range's two ends.
                let mut viewed = self.u.shallow(object_ty);
                if let Ty::Ref { to, .. } = *self.u.get(viewed) {
                    viewed = self.u.shallow(to);
                }
                // `items.len` of something with a method of that name, not a field.
                if matches!(
                    self.u.get(viewed),
                    Ty::Array { .. } | Ty::Slice(_) | Ty::Str
                ) && ArrayMethod::from_name(&field.name).is_some()
                {
                    self.errors.push(TypeError::FieldIsMethod {
                        ty: self.describe(object_ty),
                        name: field.name.to_string(),
                        span: field.span.clone(),
                    });
                    return error;
                }
                if let Ty::Range(int) = *self.u.get(viewed) {
                    let index = match &*field.name {
                        "start" => 0,
                        "end" => 1,
                        _ => {
                            self.errors.push(TypeError::UnknownField {
                                ty: self.u.display(viewed, self.names),
                                field: field.name.to_string(),
                                available: vec!["start".into(), "end".into()],
                                props: Vec::new(),
                                methods: Vec::new(),
                                span: field.span.clone(),
                            });
                            return error;
                        }
                    };
                    self.fields.insert(id, index);
                    return int;
                }
                let Some((owner, args)) = self.struct_of(object_ty, *object) else {
                    return error;
                };
                let DefKind::Struct(s) = self.names.def(owner).kind else {
                    unreachable!("struct_of returns structs")
                };
                if self.ast.structs[s].is_enum() {
                    return self.common_field(id, s, &args, field);
                }
                let fields = &self.ast.structs[s].fields;
                match fields.iter().position(|f| f.name.name == field.name) {
                    Some(index) => {
                        self.fields.insert(id, index);
                        self.check_field_visible(s, index, &field.span);
                        // Written in terms of the struct's type parameters: fill in its arguments.
                        let subst = self.struct_subst(s, &args);
                        self.substitute(self.structs[s.id][index], &subst)
                    }
                    None if self.names.method(owner, &field.name).is_some() => {
                        self.errors.push(TypeError::FieldIsMethod {
                            ty: self.describe(object_ty),
                            name: field.name.to_string(),
                            span: field.span.clone(),
                        });
                        error
                    }
                    None => {
                        let (available, props, methods) =
                            self.members(owner, object_ty, &field.span);
                        self.errors.push(TypeError::UnknownField {
                            ty: self.names.def(owner).name.to_string(),
                            field: field.name.to_string(),
                            available,
                            props,
                            methods,
                            span: field.span.clone(),
                        });
                        error
                    }
                }
            }

            Expr::Index { object, index } => self.index(id, *object, *index),

            Expr::Construct {
                ty: path,
                args: written,
                fields: given,
            } => {
                let Some(def) = self.names.expr(id) else {
                    given.iter().for_each(|(_, v)| {
                        self.infer(*v);
                    });
                    return error;
                };
                // A struct, or a variant with named fields: `Shape::Rect { w: 1.0, h: 2.0 }`.
                let (s, variant) = match self.names.def(def).kind {
                    DefKind::Struct(s) => (s, None),
                    DefKind::Variant { strukt, index } => (strukt, Some(index)),
                    _ => unreachable!("resolver only binds constructors to structs and variants"),
                };
                let def = self.names.strukt(s);
                // The type arguments written, `Pair<i32> { … }`; or each starts unknown, and the
                // fields' values decide them.
                let expected = self.names.struct_params(s).len();
                // `Self { … }` is the type the impl block is for, arguments and all: in
                // `impl Vector<3, f32>`, a `Vector<3, f32>`.
                let of_self = (path.segments.len() == 1 && &*path.segments[0].name == "Self")
                    .then(|| self.current_func.and_then(|f| self.names.impl_of(f)))
                    .flatten()
                    .map(|index| self.self_type(index))
                    // `Vector3 { … }`: the type the alias stands for.
                    .or_else(|| {
                        self.names
                            .expr_alias(id)
                            .map(|alias| self.alias_value(alias))
                    })
                    .and_then(|t| match self.u.get(self.u.shallow(t)) {
                        Ty::Struct(d, args) if *d == def => Some(args.clone()),
                        _ => None,
                    });
                let args: Vec<TypeId> = if let Some(args) = of_self {
                    args
                } else if written.is_empty() {
                    (0..expected).map(|_| self.u.fresh(VarKind::Any)).collect()
                } else {
                    self.lower_args(def, s, written, self.span(id))
                };
                let subst = self.struct_subst(s, &args);
                let ty = self.u.intern(Ty::Struct(def, args.clone()));
                let ast_struct = &self.ast.structs[s];
                let wrong_form = match variant {
                    // `Shape { … }`: an enum is made from one of its variants.
                    None if ast_struct.is_enum() => {
                        let example =
                            (!ast_struct.variants().is_empty()).then(|| self.variant_form(s, 0));
                        self.errors.push(TypeError::ConstructEnum {
                            name: ast_struct.name.name.to_string(),
                            example,
                            span: self.span(id),
                        });
                        true
                    }
                    Some(index) if ast_struct.variants()[index].kind != VariantKind::Named => {
                        self.errors.push(TypeError::WrongVariantForm {
                            name: self.variant_name(s, index),
                            holds: holds(ast_struct.variants()[index].kind),
                            form: self.variant_form(s, index),
                            span: self.span(id),
                        });
                        true
                    }
                    _ => false,
                };
                if wrong_form {
                    given.iter().for_each(|(_, v)| {
                        self.infer(*v);
                    });
                    return ty;
                }
                let (struct_name, declared, field_tys) = match variant {
                    Some(index) => (
                        self.variant_name(s, index),
                        &ast_struct.variants()[index].fields,
                        self.variant_fields(s, index, &args),
                    ),
                    None => {
                        let tys = (0..ast_struct.fields.len())
                            .map(|i| self.substitute(self.structs[s.id][i], &subst))
                            .collect();
                        (
                            self.names.def(def).name.to_string(),
                            &ast_struct.fields,
                            tys,
                        )
                    }
                };
                // Only C can make one: som doesn't know what's inside.
                if self.ast.structs[s].handle {
                    self.errors.push(TypeError::ConstructHandle {
                        name: struct_name,
                        span: self.span(id),
                    });
                    given.iter().for_each(|(_, v)| {
                        self.infer(*v);
                    });
                    return ty;
                }
                // Fields this code can't see: it can't set them, so it can't make one.
                let at = self.span(id);
                let hidden: Vec<usize> = match variant {
                    Some(_) => Vec::new(),
                    None => (0..declared.len())
                        .filter(|&i| {
                            let viewer = self.viewer(&at);
                            visibility::hidden_field(self.ast, self.names, s, i, viewer).is_some()
                        })
                        .collect(),
                };
                let mut seen = vec![false; declared.len()];
                for (name, value) in given {
                    match declared.iter().position(|f| f.name.name == name.name) {
                        Some(index) if seen[index] => {
                            self.errors.push(TypeError::DuplicateField {
                                field: name.name.to_string(),
                                span: name.span.clone(),
                            });
                            self.infer(*value);
                        }
                        Some(index) => {
                            seen[index] = true;
                            let expected = field_tys[index];
                            let from = self.ast.types.span(declared[index].ty).clone();
                            self.expect(*value, expected, Some(from));
                        }
                        None => {
                            self.errors.push(TypeError::UnknownField {
                                ty: struct_name.clone(),
                                field: name.name.to_string(),
                                available: declared
                                    .iter()
                                    .map(|f| f.name.name.to_string())
                                    .collect(),
                                props: Vec::new(),
                                methods: Vec::new(),
                                span: name.span.clone(),
                            });
                            self.infer(*value);
                        }
                    }
                }
                if !hidden.is_empty() {
                    self.errors.push(TypeError::ConstructHidden {
                        ty: struct_name,
                        fields: (hidden.iter())
                            .map(|&i| {
                                let name = &declared[i].name;
                                (name.name.to_string(), name.span.clone())
                            })
                            .collect(),
                        span: path.span(),
                        fixes: self.makers(def, &path.to_string(), given, &at),
                    });
                    return ty;
                }
                for (field, _) in declared.iter().zip(&seen).filter(|(_, seen)| !**seen) {
                    self.errors.push(TypeError::MissingField {
                        ty: struct_name.clone(),
                        field: field.name.name.to_string(),
                        span: self.span(id),
                    });
                }
                ty
            }

            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                // Like an `if` statement's: an optional is narrowed in the branch it holds in.
                let facts = self.check_condition(*condition);
                let (then_ty, _) = self.narrowed_during(facts.when_true, |c| c.infer(*then));
                let (otherwise_ty, _) =
                    self.narrowed_during(facts.when_false, |c| c.infer(*otherwise));
                // A branch that never finishes (`return`) has no value: the other's is it.
                let (then_leaves, otherwise_leaves) =
                    (self.expr_leaves(*then), self.expr_leaves(*otherwise));
                for (branch, leaves) in [(*then, then_leaves), (*otherwise, otherwise_leaves)] {
                    if leaves {
                        self.diverging.insert(branch);
                    }
                }
                match (then_leaves, otherwise_leaves) {
                    (true, true) => return self.u.fresh(VarKind::Any),
                    (true, false) => return otherwise_ty,
                    (false, true) => return then_ty,
                    (false, false) => {}
                }
                // `x if c else none`: either branch can be the optional the other is wrapped in.
                if self.coerce(*otherwise, then_ty, otherwise_ty) {
                    then_ty
                } else if self.coerce(*then, otherwise_ty, then_ty) {
                    otherwise_ty
                } else {
                    let from = self.span(*then);
                    self.mismatch(then_ty, otherwise_ty, self.span(*otherwise), Some(from));
                    then_ty
                }
            }

            Expr::Match { scrutinee, arms } => {
                let discarded = std::mem::take(&mut self.discarding);
                self.check_match(id, *scrutinee, arms, discarded)
            }

            Expr::Block { stmts, value } => {
                // What the block is expected to be is what its final value is.
                let expected = self.expected_here.take();
                // Like `check_stmts`, but what's narrowed in here lasts into the final value.
                let outer: HashSet<usize> = self.narrowed.iter().map(|n| n.id).collect();
                for &stmt in stmts {
                    self.check_stmt(stmt);
                }
                let ty = match value {
                    Some(value) => {
                        self.expected = expected;
                        let ty = self.infer(*value);
                        if matches!(
                            self.ast.exprs[*value],
                            Expr::Call { .. } | Expr::MethodCall { .. }
                        ) {
                            self.tail_calls.push((*value, ty));
                        }
                        ty
                    }
                    // `{ return x; }` never ends with a value: like `@unreachable`, it fits
                    // whatever's expected, as in `get(i) else { return none; }`.
                    None if self.leaves(stmts) => {
                        let ty = expected.unwrap_or_else(|| self.u.fresh(VarKind::Any));
                        self.unreachables.push(ty);
                        ty
                    }
                    None => unit,
                };
                self.narrowed.retain(|n| outer.contains(&n.id));
                ty
            }
        }
    }

    /// Checks a call of one of the methods every array has, if `receiver` is an array or a
    /// reference to one, returning its type.
    fn array_method(
        &mut self,
        call: Id<Expr>,
        receiver: Id<Expr>,
        receiver_ty: TypeId,
        method: &som_ast::Ident,
        args: &[Id<Expr>],
        as_prop: bool,
    ) -> Option<TypeId> {
        let error = self.u.table.error;
        // `[1, 2, 3].len()`: a collection nothing else decides is an array, and a string a `str`.
        match self.u.var_kind(receiver_ty) {
            Some(VarKind::Collection { element, len }) => {
                let array = self.u.intern(Ty::Array { element, len });
                let _ = self.u.unify(receiver_ty, array);
            }
            Some(VarKind::Text) => {
                let text = self.u.table.text;
                let _ = self.u.unify(receiver_ty, text);
            }
            _ => {}
        }
        let shallow = self.u.shallow(receiver_ty);
        let (array, through) = match *self.u.get(shallow) {
            Ty::Ref { mutable, to } => (self.u.shallow(to), Some(mutable)),
            _ => (shallow, None),
        };
        let (element, is_slice) = match *self.u.get(array) {
            Ty::Array { element, .. } => (element, false),
            Ty::Slice(element) => (element, true),
            // Text is its bytes.
            Ty::Str => (self.u.table.u8, true),
            _ => return None,
        };
        let infer_args = |c: &mut Self| {
            for &arg in args {
                c.infer(arg);
            }
        };
        let is_text = *self.u.get(array) == Ty::Str;
        // `bytes` is text's own.
        let kind = ArrayMethod::from_name(&method.name)
            .filter(|&kind| kind != ArrayMethod::Bytes || is_text);
        let Some(kind) = kind else {
            // What every array has: `len`, read, and its methods.
            let methods = (ArrayMethod::ALL.iter())
                .map(|&(name, _)| name)
                .filter(|&name| name != "len" && (name != "bytes" || is_text))
                .map(String::from)
                .collect();
            self.errors.push(TypeError::UnknownMethod {
                ty: self.describe(array),
                method: method.name.to_string(),
                span: method.span.clone(),
                methods,
                props: vec!["len".into()],
                fields: Vec::new(),
                bare_call: self.bare_call(call, method, args),
            });
            infer_args(self);
            return Some(error);
        };
        // `len` is read as a property, `items.len`.
        if kind == ArrayMethod::Len && !as_prop {
            self.errors.push(TypeError::MethodIsProp {
                ty: self.describe(array),
                name: method.name.to_string(),
                span: method.span.clone(),
                parens: args
                    .is_empty()
                    .then(|| method.span.after().merge(&self.span(call).after())),
            });
            infer_args(self);
            return Some(error);
        }
        if args.len() != kind.arity() {
            self.errors.push(TypeError::WrongArgCount {
                name: method.name.to_string(),
                expected: kind.arity(),
                at_least: false,
                found: args.len(),
                span: self.span(call),
            });
            infer_args(self);
            return Some(error);
        }
        self.array_methods.insert(call, kind);

        // A range of any integer type, or any integer, says which elements.
        if matches!(kind, ArrayMethod::Slice | ArrayMethod::SliceMut) {
            let int = self.u.fresh(VarKind::Int);
            let range = self.u.intern(Ty::Range(int));
            match &self.ast.exprs[args[0]] {
                // `1..` and `..2`: the ends left out are the slice's own.
                Expr::Range { start, end } if start.is_none() || end.is_none() => {
                    for &bound in start.iter().chain(end) {
                        self.expect(bound, int, None);
                    }
                    self.exprs[args[0].id] = Some(range);
                }
                _ => self.expect(args[0], range, None),
            }
        } else if let Some(&index) = args.first() {
            let int = self.u.fresh(VarKind::Int);
            self.expect(index, int, None);
        }
        if kind == ArrayMethod::Set {
            self.expect(args[1], element, None);
        }
        // Changing an element changes the array: it has to be changeable, or behind a `&mut`.
        if matches!(
            kind,
            ArrayMethod::Set | ArrayMethod::GetMut | ArrayMethod::SliceMut
        ) {
            let borrow = kind != ArrayMethod::Set;
            match through {
                Some(true) => {}
                Some(false) if *self.u.get(array) == Ty::Str => {
                    self.errors.push(TypeError::ReadOnlyStr {
                        span: self.span(receiver),
                    })
                }
                Some(false) if is_slice => self.errors.push(TypeError::ReadOnlySlice {
                    span: self.span(receiver),
                }),
                Some(false) => self.errors.push(TypeError::BehindSharedReference {
                    span: self.span(receiver),
                    borrow,
                }),
                None => self.check_mutable(receiver, borrow),
            }
        }
        if kind == ArrayMethod::Get && !self.is_copy(element) {
            self.errors.push(TypeError::NotCopyable {
                ty: self.describe(element),
                span: method.span.clone(),
                repeat: false,
            });
        }

        Some(match kind {
            // The length is a constant, usable as any integer type.
            ArrayMethod::Len => self.u.fresh(VarKind::Int),
            ArrayMethod::Get => self.u.intern(Ty::Optional(element)),
            ArrayMethod::GetRef | ArrayMethod::GetMut => {
                let reference = self.u.intern(Ty::Ref {
                    mutable: kind == ArrayMethod::GetMut,
                    to: element,
                });
                self.u.intern(Ty::Optional(reference))
            }
            ArrayMethod::Set => self.u.table.bool,
            ArrayMethod::Slice if is_text => {
                let str = self.u.table.str_ref;
                self.u.intern(Ty::Optional(str))
            }
            ArrayMethod::Slice | ArrayMethod::SliceMut => {
                let slice = self.u.intern(Ty::Slice(element));
                let view = self.u.intern(Ty::Ref {
                    mutable: kind == ArrayMethod::SliceMut,
                    to: slice,
                });
                self.u.intern(Ty::Optional(view))
            }
            ArrayMethod::Bytes => {
                let u8 = self.u.table.u8;
                let bytes = self.u.intern(Ty::Slice(u8));
                self.u.intern(Ty::Ref {
                    mutable: false,
                    to: bytes,
                })
            }
        })
    }

    /// A call `callee(args)` naming function `first` and others with its name: the one whose
    /// parameters fit the arguments and whose result fits `expected`, where that's known; of
    /// several, one that isn't generic. Returns the call's type.
    fn overloaded_call(
        &mut self,
        call: Id<Expr>,
        callee: &som_ast::Path,
        first: DefId,
        owner_args: &[Id<TypeExpr>],
        written: &[Id<TypeExpr>],
        args: &[Id<Expr>],
        expected: Option<TypeId>,
    ) -> TypeId {
        // The arguments decide which, so they're worked out first, once.
        let found: Vec<TypeId> = args.iter().map(|&a| self.infer(a)).collect();
        // So are the type arguments written out: only one taking as many fits.
        let written: Vec<TypeId> = written.iter().map(|&t| self.lower(t)).collect();
        // `Vector3::zero()`: one of the type the alias stands for.
        let aliased = (self.names.expr_alias(call)).map(|alias| self.alias_value(alias));
        // `Alloc<i32>::with_capacity(10)`: one of the type with the arguments written.
        let owner_written = self.owner_written(call, callee, first, owner_args);
        let aliased = aliased.or(owner_written);
        let mut fitting = Vec::new();
        // Ones the arguments fit as they are; only if there are none, ones they fit converted.
        for converting in [false, true] {
            for def in self.names.overloads(first) {
                let DefKind::Func(func) = self.names.def(def).kind else {
                    continue;
                };
                let (sig, type_args) = self.instantiate(func);
                if !written.is_empty() {
                    let inherited = type_args.len() - self.own_type_params(func).len();
                    if written.len() != type_args.len() - inherited {
                        continue;
                    }
                    for (&var, &ty) in type_args[inherited..].iter().zip(&written) {
                        let _ = self.u.unify(var, ty);
                    }
                }
                let variadic = self.ast.funcs[func].variadic;
                let count_ok = match variadic {
                    true => found.len() >= sig.params.len(),
                    false => found.len() == sig.params.len(),
                };
                let owner_fits = match (aliased, self.owner_type(func, &type_args)) {
                    (Some(aliased), Some(owner)) => self.u.could_fit(aliased, owner),
                    _ => true,
                };
                let fits = count_ok
                    && owner_fits
                    && self.args_fit(&sig.params, &found, converting)
                    && expected.is_none_or(|e| self.u.could_fit(e, sig.ret));
                if fits {
                    fitting.push((def, func, sig, type_args));
                }
            }
            if !fitting.is_empty() {
                break;
            }
        }
        // Of several, one that isn't generic fits most closely.
        if fitting.len() > 1 {
            let plain: Vec<_> = (fitting.iter())
                .filter(|(.., type_args)| type_args.is_empty())
                .cloned()
                .collect();
            if plain.len() == 1 {
                fitting = plain;
            }
        }
        // Of several, the ones taking each argument as it is, not wrapped in an optional;
        // then those taking each number nothing's decided as what it would be anyway, an
        // `i32` or `f64`: `string::from(100)` is the `From<i32>` one.
        for pass in 0..2 {
            if fitting.len() < 2 {
                break;
            }
            let kept: Vec<_> = (fitting.iter())
                .filter(|(_, _, sig, _)| {
                    (sig.params.iter().zip(&found)).all(|(&param, &arg)| match pass {
                        0 => {
                            matches!(self.u.get(self.u.shallow(arg)), Ty::Optional(_))
                                || !matches!(self.u.get(self.u.shallow(param)), Ty::Optional(_))
                        }
                        _ => self.fits_as_default(param, arg),
                    })
                })
                .cloned()
                .collect();
            if !kept.is_empty() {
                fitting = kept;
            }
        }
        let candidates = |c: &Self, defs: Vec<DefId>| -> Vec<(String, Span)> {
            (defs.into_iter())
                .filter_map(|d| match c.names.def(d).kind {
                    DefKind::Func(f) => Some((c.signature_text(f), c.names.def(d).span.clone())),
                    _ => None,
                })
                .collect()
        };
        if fitting.is_empty() {
            self.errors.push(TypeError::NoMatchingOverload {
                name: callee.to_string(),
                args: found.iter().map(|&t| self.describe(t)).collect(),
                candidates: candidates(self, self.names.overloads(first)),
                span: self.span(call),
            });
            return self.u.table.error;
        }
        // Several fit: what the result is used as later may decide.
        if fitting.len() > 1 {
            let result = self.u.fresh(VarKind::Any);
            self.pending_calls.push(PendingCall {
                call,
                name: callee.to_string(),
                args: args.to_vec(),
                found,
                fitting,
                result,
            });
            return result;
        }
        let (def, func, sig, type_args) = fitting.pop().expect("exactly one");
        if let Some(owner) = self.owner_type(func, &type_args) {
            self.fit_alias(call, owner);
            if let Some(written) = owner_written
                && self.u.unify(owner, written).is_err()
            {
                self.mismatch(owner, written, self.span(call), None);
            }
        }
        self.finish_call(call, def, func, &sig, type_args, args, &found);
        sig.ret
    }

    /// Whether arguments of types `found` fit parameters `params` as they are, or with
    /// `converting`, also through `From`.
    fn args_fit(&mut self, params: &[TypeId], found: &[TypeId], converting: bool) -> bool {
        (params.iter().zip(found))
            .all(|(&p, &a)| self.u.could_fit(p, a) || (converting && self.could_convert(p, a)))
    }

    /// A method call `receiver.name(args)` where several methods have that name: the one
    /// whose `self` takes the receiver, whose other parameters fit the arguments and whose
    /// result fits `expected`. Of several, the one taking the receiver as it is, then by `&`,
    /// then by `&mut`: `list.get(0)` is the `&self` one unless a `&mut` result is expected.
    #[allow(clippy::too_many_arguments)]
    fn overloaded_method(
        &mut self,
        call: Id<Expr>,
        name: &str,
        first: DefId,
        receiver: Id<Expr>,
        receiver_ty: TypeId,
        args: &[Id<Expr>],
        expected: Option<TypeId>,
    ) -> TypeId {
        let found: Vec<TypeId> = args.iter().map(|&a| self.infer(a)).collect();
        // Each fitting method, and how much the receiver has to change for it: 0 as it is,
        // 1 borrowed, 2 borrowed as `&mut`.
        let mut fitting = Vec::new();
        // If several fit numbers nothing has decided yet, `[1.0, 2.0].normalize()`, they're
        // settled to `f64` and `i32`, as they would be anyway, and it's chosen again.
        for attempt in 0..2 {
            if attempt == 1 {
                let mut settled = self.default_numbers(receiver_ty);
                for &ty in &found {
                    settled |= self.default_numbers(ty);
                }
                if !settled {
                    break;
                }
                fitting.clear();
            }
            // Ones the arguments fit as they are; only if there are none, ones they fit converted.
            for converting in [false, true] {
                for def in self.names.overloads(first) {
                    let DefKind::Func(func) = self.names.def(def).kind else {
                        continue;
                    };
                    let (sig, type_args) = self.instantiate(func);
                    let Some((&self_ty, rest)) = sig.params.split_first() else {
                        continue;
                    };
                    let Some(cost) = self.receiver_cost(self_ty, receiver_ty) else {
                        continue;
                    };
                    // The receiver decides the impl's parameters, so `.read()` of an `Alloc<T>`
                    // gives `T`s, not anything. Only a receiver without unknowns, so the only
                    // variables bound are this candidate's own.
                    let (wanted, given) = (self.referent(self_ty), self.referent(receiver_ty));
                    if !self.has_unknowns(given) {
                        let _ = self.u.unify(wanted, given);
                    }
                    let fits = rest.len() == found.len()
                        && self.args_fit(rest, &found, converting)
                        && expected.is_none_or(|e| self.u.could_fit(e, sig.ret));
                    if fits {
                        fitting.push((cost, def, func, sig, type_args));
                    }
                }
                if !fitting.is_empty() {
                    break;
                }
            }
            if let Some(cheapest) = fitting.iter().map(|f| f.0).min() {
                fitting.retain(|f| f.0 == cheapest);
            }
            if fitting.len() <= 1 {
                break;
            }
        }
        let candidates = |c: &Self, defs: Vec<DefId>| -> Vec<(String, Span)> {
            (defs.into_iter())
                .filter_map(|d| match c.names.def(d).kind {
                    DefKind::Func(f) => Some((c.signature_text(f), c.names.def(d).span.clone())),
                    _ => None,
                })
                .collect()
        };
        if fitting.len() != 1 {
            let error = match fitting.is_empty() {
                true => TypeError::NoMatchingOverload {
                    name: name.to_string(),
                    args: std::iter::once(receiver_ty)
                        .chain(found.iter().copied())
                        .map(|t| self.describe(t))
                        .collect(),
                    candidates: candidates(self, self.names.overloads(first)),
                    span: self.span(call),
                },
                false => TypeError::AmbiguousCall {
                    name: name.to_string(),
                    candidates: candidates(self, fitting.iter().map(|f| f.1).collect()),
                    span: self.span(call),
                },
            };
            self.errors.push(error);
            return self.u.table.error;
        }
        let (_, def, func, sig, type_args) = fitting.pop().expect("exactly one");
        self.methods.insert(call, def);
        if let Expr::MethodCall { method, .. } = &self.ast.exprs[call] {
            self.check_visible(def, &method.span);
        }
        self.require_bounds(func, &type_args, self.span(call));
        if !type_args.is_empty() {
            self.type_args.insert(call, type_args);
        }
        let self_ty = sig.params[0];
        self.receive(receiver, self_ty, receiver_ty);
        for (i, ((&arg, &param), &ty)) in args.iter().zip(&sig.params[1..]).zip(&found).enumerate()
        {
            if !self.coerce(arg, param, ty) {
                let declared = &self.ast.funcs[func].params[i + 1];
                let from = declared.ty.map(|t| self.ast.types.span(t).clone());
                self.mismatch_at(arg, param, ty, from);
            }
        }
        sig.ret
    }

    /// What `ty` refers to, if it's a reference; otherwise `ty`.
    fn referent(&self, ty: TypeId) -> TypeId {
        match self.u.get(self.u.shallow(ty)) {
            Ty::Ref { to, .. } => *to,
            _ => ty,
        }
    }

    /// Whether `ty` has a type in it that isn't decided yet.
    fn has_unknowns(&self, ty: TypeId) -> bool {
        let ty = self.u.shallow(ty);
        match self.u.get(ty) {
            Ty::Var(_) => true,
            t => t.children().into_iter().any(|c| self.has_unknowns(c)),
        }
    }

    /// How much a receiver of type `found` changes to be a `self` of type `expected`, without
    /// changing anything: 0 as it is, 1 borrowed, 2 borrowed as `&mut`, 1 copied out of a
    /// reference; `None` if it can't be.
    fn receiver_cost(&self, expected: TypeId, found: TypeId) -> Option<u8> {
        let wanted = self.u.get(self.u.shallow(expected)).clone();
        let given = self.u.get(self.u.shallow(found)).clone();
        match (wanted, given) {
            (Ty::Ref { mutable, to }, given) if !matches!(given, Ty::Ref { .. } | Ty::Error) => {
                self.u
                    .could_fit(to, found)
                    .then_some(if mutable { 2 } else { 1 })
            }
            (
                Ty::Ref { mutable: m, to },
                Ty::Ref {
                    mutable: n,
                    to: given,
                },
            ) => (m == n && self.u.could_fit(to, given)).then_some(0),
            (wanted, Ty::Ref { to, .. }) if !matches!(wanted, Ty::Var(_)) => {
                (self.u.could_fit(expected, to) && self.is_copy(to)).then_some(1)
            }
            _ => self.u.could_fit(expected, found).then_some(0),
        }
    }

    /// Decides the calls more than one function fitted, now that more is known about their
    /// results: `let a = make(); a + 1` takes the `make` giving a number. One decided can tell
    /// another's result, so until nothing changes; what's still undecided is reported.
    fn resolve_pending_calls(&mut self) {
        let mut pending = std::mem::take(&mut self.pending_calls);
        loop {
            let mut progress = false;
            let mut undecided = Vec::new();
            for mut p in pending {
                let fits = |c: &Self, sig: &Signature| {
                    c.u.could_fit(p.result, sig.ret)
                        && sig
                            .params
                            .iter()
                            .zip(&p.found)
                            .all(|(&x, &y)| c.u.could_fit(x, y))
                };
                p.fitting.retain(|(.., sig, _)| fits(self, sig));
                if p.fitting.len() > 1 {
                    let plain: Vec<_> = (p.fitting.iter())
                        .filter(|(.., type_args)| type_args.is_empty())
                        .cloned()
                        .collect();
                    if plain.len() == 1 {
                        p.fitting = plain;
                    }
                }
                if p.fitting.len() == 1 {
                    let (def, func, sig, type_args) = p.fitting.pop().expect("exactly one");
                    let _ = self.u.unify(p.result, sig.ret);
                    self.finish_call(p.call, def, func, &sig, type_args, &p.args, &p.found);
                    progress = true;
                } else {
                    undecided.push(p);
                }
            }
            pending = undecided;
            if !progress || pending.is_empty() {
                break;
            }
        }
        for p in pending {
            let candidates = (p.fitting.iter())
                .map(|(def, func, ..)| {
                    (
                        self.signature_text(*func),
                        self.names.def(*def).span.clone(),
                    )
                })
                .collect();
            self.errors.push(TypeError::AmbiguousCall {
                name: p.name,
                candidates,
                span: self.span(p.call),
            });
            // Already reported: don't also say its result can't be inferred.
            self.u.poison(p.result);
        }
    }

    /// Records that `call` calls `func` (`def`), with signature `sig` and type arguments
    /// `type_args`, and checks its arguments, of types `found`, against its parameters.
    #[allow(clippy::too_many_arguments)]
    fn finish_call(
        &mut self,
        call: Id<Expr>,
        def: DefId,
        func: Id<Func>,
        sig: &Signature,
        type_args: Vec<TypeId>,
        args: &[Id<Expr>],
        found: &[TypeId],
    ) {
        self.callees.insert(call, def);
        self.require_bounds(func, &type_args, self.span(call));
        if !type_args.is_empty() {
            self.type_args.insert(call, type_args);
        }
        for (i, ((&arg, &param), &ty)) in args.iter().zip(&sig.params).zip(found).enumerate() {
            let declared = &self.ast.funcs[func].params[i];
            if declared.out && !matches!(self.ast.exprs[arg], Expr::Out(_)) {
                self.errors.push(TypeError::NeedsOut {
                    param: declared.name.name.to_string(),
                    span: self.span(arg),
                    declared_at: declared.name.span.clone(),
                });
                continue;
            }
            if !self.coerce(arg, param, ty) {
                let from = declared.ty.map(|t| self.ast.types.span(t).clone());
                self.mismatch_at(arg, param, ty, from);
            }
        }
    }

    /// For a function value naming `first` and others with its name: the one whose type fits
    /// `expected`, or else the one that isn't generic. `None` if `first` is alone.
    fn pick_function(
        &mut self,
        expr: Id<Expr>,
        first: DefId,
        expected: Option<TypeId>,
    ) -> Option<Id<Func>> {
        let all = self.names.overloads(first);
        if all.len() < 2 {
            return None;
        }
        let funcs: Vec<(DefId, Id<Func>)> = (all.into_iter())
            .filter_map(|d| match self.names.def(d).kind {
                DefKind::Func(f) => Some((d, f)),
                _ => None,
            })
            .collect();
        let fits = |c: &mut Self, f: Id<Func>| {
            let (sig, _) = c.instantiate(f);
            let ty = c.u.intern(Ty::Fn {
                params: sig.params,
                ret: sig.ret,
            });
            expected.is_none_or(|e| c.u.could_fit(e, ty))
        };
        let mut chosen = None;
        for &(def, f) in &funcs {
            if fits(self, f) && (chosen.is_none() || self.names.type_params(f).is_empty()) {
                chosen = Some((def, f));
            }
        }
        let (def, f) = chosen.unwrap_or(funcs[0]);
        self.callees.insert(expr, def);
        Some(f)
    }

    /// Reports functions sharing a name that can't be told apart: neither generic, and taking
    /// and giving the same types.
    fn check_overloads(&mut self) {
        let firsts: Vec<DefId> = (self.ast.funcs.iter())
            .filter_map(|(id, _)| self.names.try_func(id))
            .filter(|&def| self.names.overloads(def).len() > 1)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        for first in firsts {
            let funcs: Vec<(Id<Func>, Span)> = (self.names.overloads(first).into_iter())
                .filter_map(|d| match self.names.def(d).kind {
                    DefKind::Func(f) => Some((f, self.names.def(d).span.clone())),
                    _ => None,
                })
                .collect();
            for (i, (f, span)) in funcs.iter().enumerate() {
                // The same types, as far as they're known: an integer not yet decided counts as
                // the same as any integer.
                let alike =
                    |c: &Self, x: TypeId, y: TypeId| c.u.could_fit(x, y) && c.u.could_fit(y, x);
                let same = funcs[..i].iter().find(|(g, _)| {
                    let (fs, gs) = (&self.sigs[f.id], &self.sigs[g.id]);
                    self.names.type_params(*f).is_empty()
                        && self.names.type_params(*g).is_empty()
                        && fs.params.len() == gs.params.len()
                        && (fs.params.iter().zip(&gs.params)).all(|(&x, &y)| alike(self, x, y))
                        && alike(self, fs.ret, gs.ret)
                });
                if let Some((_, previous)) = same {
                    self.errors.push(TypeError::DuplicateOverload {
                        name: self.ast.funcs[*f].name.name.to_string(),
                        span: span.clone(),
                        previous: previous.clone(),
                    });
                    continue;
                }
                // `view(&self)` and `view(&mut self)`: which is called would depend on whether
                // the caller has a `&mut`, so the one that changes is named apart.
                let only_mut_differs = funcs[..i].iter().find_map(|&(g, ref previous)| {
                    let (fs, gs) = (&self.sigs[f.id], &self.sigs[g.id]);
                    let comparable = self.names.type_params(*f).is_empty()
                        && self.names.type_params(g).is_empty()
                        && fs.params.len() == gs.params.len();
                    if !comparable {
                        return None;
                    }
                    let mut mutable = None;
                    for (&x, &y) in fs.params.iter().zip(&gs.params) {
                        if alike(self, x, y) {
                            continue;
                        }
                        let (
                            &Ty::Ref {
                                mutable: mx,
                                to: tx,
                            },
                            &Ty::Ref {
                                mutable: my,
                                to: ty,
                            },
                        ) = (self.u.get(self.u.shallow(x)), self.u.get(self.u.shallow(y)))
                        else {
                            return None;
                        };
                        if mx == my || !alike(self, tx, ty) {
                            return None;
                        }
                        mutable = Some(if mx { *f } else { g });
                    }
                    mutable.map(|m| (m, previous.clone()))
                });
                if let Some((mutable, previous)) = only_mut_differs {
                    let name = &self.ast.funcs[mutable].name;
                    let renamed = format!("{}_mut", name.name);
                    self.errors.push(TypeError::MutOverload {
                        name: name.name.to_string(),
                        span: name.span.clone(),
                        previous: if mutable == *f {
                            previous
                        } else {
                            span.clone()
                        },
                        fix: Fix::replace(
                            msg!["rename it ", message::code(&renamed)],
                            name.span.clone(),
                            renamed,
                        ),
                    });
                }
            }
        }
    }

    /// `name<T>(a: i32, b: T) -> T`, for messages.
    fn signature_text(&self, func: Id<Func>) -> String {
        let f = &self.ast.funcs[func];
        let generics: Vec<&str> = f.generics.iter().map(|g| &*g.name.name).collect();
        let generics = match generics.is_empty() {
            true => String::new(),
            false => format!("<{}>", generics.join(", ")),
        };
        let params: Vec<String> = (f.params.iter().zip(&self.sigs[func.id].params))
            .map(|(p, &t)| format!("{}: {}", p.name, self.u.display(t, self.names)))
            .collect();
        let ret = self.sigs[func.id].ret;
        let ret = match self.u.get(self.u.shallow(ret)) {
            Ty::Unit => String::new(),
            _ => format!(" -> {}", self.u.display(ret, self.names)),
        };
        format!("{}{generics}({}){ret}", f.name, params.join(", "))
    }

    /// Checks a call of one of the methods every typed pointer has, if the receiver is one,
    /// returning its type.
    fn pointer_method(
        &mut self,
        call: Id<Expr>,
        receiver_ty: TypeId,
        method: &som_ast::Ident,
        args: &[Id<Expr>],
    ) -> Option<TypeId> {
        let Ty::Ptr(element) = *self.u.get(self.u.shallow(receiver_ty)) else {
            return None;
        };
        let (error, unit, bool) = (self.u.table.error, self.u.table.unit, self.u.table.bool);
        let Some(kind) = PointerMethod::from_name(&method.name) else {
            let methods = (PointerMethod::ALL.iter())
                .map(|&(name, _)| name.to_string())
                .collect();
            self.errors.push(TypeError::UnknownMethod {
                ty: self.describe(receiver_ty),
                method: method.name.to_string(),
                span: method.span.clone(),
                methods,
                props: Vec::new(),
                fields: Vec::new(),
                bare_call: self.bare_call(call, method, args),
            });
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return Some(error);
        };
        if args.len() != kind.arity() {
            self.errors.push(TypeError::WrongArgCount {
                name: method.name.to_string(),
                expected: kind.arity(),
                at_least: false,
                found: args.len(),
                span: self.span(call),
            });
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return Some(error);
        }
        self.pointer_methods.insert(call, kind);
        // The first argument is a count or an index.
        let usize = self.u.table.usize;
        if let Some(&first) = args.first() {
            self.expect(first, usize, None);
        }
        Some(match kind {
            // It changes the pointer it's called on into the new one.
            PointerMethod::Resize => {
                if let Expr::MethodCall { receiver, .. } = self.ast.exprs[call] {
                    self.check_mutable(receiver, false);
                }
                unit
            }
            PointerMethod::Free => unit,
            PointerMethod::Read => element,
            PointerMethod::Write => {
                self.expect(args[1], element, None);
                unit
            }
            PointerMethod::At | PointerMethod::AtMut => self.u.intern(Ty::Ref {
                mutable: kind == PointerMethod::AtMut,
                to: element,
            }),
            PointerMethod::View | PointerMethod::ViewMut => {
                let slice = self.u.intern(Ty::Slice(element));
                self.u.intern(Ty::Ref {
                    mutable: kind == PointerMethod::ViewMut,
                    to: slice,
                })
            }
            PointerMethod::IsNull => bool,
        })
    }

    /// Makes method call receiver `receiver`, of type `found`, the `self` of a method taking
    /// `expected`, returning whether it can be: as it is; borrowed, when the method takes a
    /// reference to what it is (`a.len()` for `self: &List`); or copied out of a reference to
    /// what the method takes, if that copies.
    fn receive(&mut self, receiver: Id<Expr>, expected: TypeId, found: TypeId) -> bool {
        let wanted = self.u.get(self.u.shallow(expected)).clone();
        let given = self.u.get(self.u.shallow(found)).clone();
        let adjusted = match (wanted, given) {
            (Ty::Ref { mutable, to }, given) if !matches!(given, Ty::Ref { .. } | Ty::Error) => {
                if self.u.unify(to, found).is_err() {
                    return false;
                }
                // Like `&mut a`: it can change anything inside `a`, and `a` has to allow that.
                if mutable {
                    if let Some(borrowed) = self.key(receiver) {
                        self.forget(|key| key.within(&borrowed) && !key.same(&borrowed));
                    }
                    self.check_mutable(receiver, true);
                }
                Receiver::Borrow { mutable }
            }
            // `self.get()` with `&mut self`, for `fn get(&self)`: lent as `&`.
            (
                Ty::Ref {
                    mutable: false,
                    to: wanted,
                },
                Ty::Ref {
                    mutable: true,
                    to: given,
                },
            ) => {
                if self.u.unify(wanted, given).is_err() {
                    return false;
                }
                Receiver::Share
            }
            (wanted, Ty::Ref { to, .. }) if !matches!(wanted, Ty::Ref { .. } | Ty::Var(_)) => {
                if self.u.unify(expected, to).is_err() {
                    return false;
                }
                // Taking it would move it out from behind the reference.
                if !self.is_copy(to) {
                    self.errors.push(TypeError::ReceiverBehindReference {
                        ty: self.describe(to),
                        span: self.span(receiver),
                    });
                    return true;
                }
                Receiver::Deref
            }
            _ => return self.u.unify(expected, found).is_ok(),
        };
        self.receivers.insert(receiver, adjusted);
        true
    }

    /// Whether values of `ty` are copied rather than moved. A type that isn't known yet is
    /// taken to be.
    fn is_copy(&self, ty: TypeId) -> bool {
        match *self.u.get(self.u.shallow(ty)) {
            Ty::Struct(def, _) => match self.names.def(def).kind {
                DefKind::Struct(s) => self.ast.structs[s].copy || self.implements_copy(def),
                _ => true,
            },
            Ty::Ref { mutable: true, .. } => false,
            Ty::Optional(inner) | Ty::Array { element: inner, .. } => self.is_copy(inner),
            // Whatever it stands for might move: only copied if it's required to be `Copy`.
            Ty::Param(def) => (self.names.lang(som_resolve::LangTrait::Copy))
                .is_some_and(|copy| self.names.bounds(def).contains(&copy)),
            _ => true,
        }
    }

    fn check_args(
        &mut self,
        call: Id<Expr>,
        name: &str,
        func: Id<Func>,
        params: &[TypeId],
        args: &[Id<Expr>],
    ) {
        self.check_args_from(call, name, func, 0, params, args);
    }

    /// Checks `args` against `params`, which are `func`'s parameters starting at `first`.
    fn check_args_from(
        &mut self,
        call: Id<Expr>,
        name: &str,
        func: Id<Func>,
        first: usize,
        params: &[TypeId],
        args: &[Id<Expr>],
    ) {
        // A variadic extern takes any number of extra arguments after its parameters.
        let variadic = self.ast.funcs[func].variadic;
        let count_ok = if variadic {
            args.len() >= params.len()
        } else {
            args.len() == params.len()
        };
        if !count_ok {
            self.errors.push(TypeError::WrongArgCount {
                name: name.to_string(),
                expected: params.len(),
                at_least: variadic,
                found: args.len(),
                span: self.span(call),
            });
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return;
        }
        for (i, (&arg, &param)) in args.iter().zip(params).enumerate() {
            let declared = &self.ast.funcs[func].params[first + i];
            if declared.out && !matches!(self.ast.exprs[arg], Expr::Out(_)) {
                self.errors.push(TypeError::NeedsOut {
                    param: declared.name.name.to_string(),
                    span: self.span(arg),
                    declared_at: declared.name.span.clone(),
                });
                self.infer(arg);
                continue;
            }
            let from = declared.ty.map(|t| self.ast.types.span(t).clone());
            self.expect(arg, param, from);
        }
        // C reads a variadic function's extra arguments as C types: text has to be a `cstr`.
        for &extra in &args[params.len()..] {
            let ty = self.infer(extra);
            if self.u.var_kind(ty) == Some(VarKind::Text) {
                let cstr = self.u.table.cstr;
                let _ = self.u.unify(ty, cstr);
            } else if self.u.shallow(ty) == self.u.table.text {
                self.errors.push(TypeError::StrToC {
                    span: self.span(extra),
                });
            } else {
                // C reads the bytes it's given as the type its format says: numbers, pointers
                // and `cstr`. An optional, a reference or a struct would be read as garbage.
                let shown = self.u.shallow(ty);
                let what = match self.u.get(shown) {
                    Ty::Optional(_) => Some(NotForC::Optional),
                    Ty::Ref { .. } => Some(NotForC::Reference),
                    Ty::Struct(..) | Ty::Array { .. } | Ty::Slice(_) | Ty::Unit | Ty::Fn { .. } => {
                        Some(NotForC::Other)
                    }
                    _ => None,
                };
                if let Some(what) = what {
                    self.errors.push(TypeError::NotForC {
                        ty: self.describe(ty),
                        what,
                        span: self.span(extra),
                    });
                }
            }
        }
    }

    /// The struct `ty` is, looking through one reference, and its type arguments. Reports an
    /// error if it isn't one.
    /// The definition built-in type `ty`'s own methods hang on, looking through a reference;
    /// an integer not settled yet is an `i32`'s, a float an `f64`'s.
    fn primitive_owner(&self, ty: TypeId) -> Option<DefId> {
        let mut ty = self.u.shallow(ty);
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
        }
        let primitive = match self.u.var_kind(ty) {
            Some(VarKind::Int) => som_ast::Primitive::I32,
            Some(VarKind::Float) => som_ast::Primitive::F64,
            Some(VarKind::Text) => som_ast::Primitive::Str,
            _ => crate::traits::primitive_of(self.u.get(ty))?,
        };
        self.names.primitive(primitive)
    }

    fn struct_of(&mut self, ty: TypeId, expr: Id<Expr>) -> Option<(DefId, Vec<TypeId>)> {
        let outer = self.u.shallow(ty);
        let mut ty = outer;
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
        }
        // `&mut x` of an `x` that's already a reference: say so, not that it isn't a struct.
        if let Ty::Ref { to, .. } = *self.u.get(ty)
            && matches!(self.u.get(self.u.shallow(to)), Ty::Struct(..))
        {
            let span = self.span(expr);
            self.errors.push(TypeError::ReferenceToReference {
                object: span.get_text().to_string(),
                ty: self.describe(outer),
                span,
            });
            return None;
        }
        match self.u.get(ty) {
            Ty::Struct(def, args) => Some((*def, args.clone())),
            Ty::Error => None,
            Ty::Var(_) if self.u.var_kind(ty) == Some(VarKind::Any) => {
                self.errors.push(TypeError::CannotInfer {
                    span: self.span(expr),
                });
                None
            }
            // `get(i).value`: it may hold nothing, which is what to say.
            Ty::Optional(_) => {
                let span = self.span(expr);
                self.errors.push(TypeError::MemberOfOptional {
                    object: span.get_text().to_string(),
                    ty: self.describe(ty),
                    variable: matches!(self.ast.exprs[expr], Expr::Var(_)),
                    span,
                });
                None
            }
            _ => {
                self.errors.push(TypeError::NotAStruct {
                    ty: self.describe(ty),
                    span: self.span(expr),
                });
                None
            }
        }
    }

    /// Reports it if the place `expr` can't be changed: assigned to, or with `borrow`, borrowed
    /// as `&mut`.
    fn check_mutable(&mut self, expr: Id<Expr>, borrow: bool) {
        // `xs[i].x = 1`: through `at_mut`, which checks `xs` can change.
        let mut place = expr;
        loop {
            match self.ast.exprs[place] {
                Expr::Field { object, .. }
                    if !self.is_prop(place) && self.reference(object).is_none() =>
                {
                    place = object
                }
                Expr::Index { .. } if self.methods.contains_key(&place) => {
                    self.index_mut(place);
                    return;
                }
                _ => break,
            }
        }
        let error = match self.immutable(expr) {
            Some(Immutable::Variable(def)) => {
                let def = self.names.def(def);
                TypeError::NotMutable {
                    name: def.name.to_string(),
                    span: self.span(expr),
                    declared_at: def.span.clone(),
                    borrow,
                }
            }
            Some(Immutable::Const(def)) => {
                let def = self.names.def(def);
                TypeError::ConstChanged {
                    name: def.name.to_string(),
                    span: self.span(expr),
                    defined_at: def.span.clone(),
                    borrow,
                }
            }
            Some(Immutable::BehindSharedReference) => TypeError::BehindSharedReference {
                span: self.span(expr),
                borrow,
            },
            Some(Immutable::Shared) => TypeError::SharedChanged {
                span: self.span(expr),
                borrow,
            },
            None => return,
        };
        self.errors.push(error);
    }

    /// Why the place `expr` can't be changed, if it can't. `out` parameters and variables can
    /// be: being filled in is what they're for. Anything that isn't a place, like a call's
    /// result, is a fresh value.
    fn immutable(&self, expr: Id<Expr>) -> Option<Immutable> {
        match &self.ast.exprs[expr] {
            Expr::Var(_) => {
                let def = self.names.expr(expr)?;
                let mutable = match self.names.def(def).kind {
                    DefKind::Const(_) => return Some(Immutable::Const(def)),
                    DefKind::Local(stmt) => matches!(
                        self.ast.stmts[stmt],
                        Stmt::Let { mutable: true, .. } | Stmt::For { mutable: true, .. }
                    ),
                    DefKind::Param { func, index } => {
                        let param = &self.ast.funcs[func].params[index];
                        param.mutable || param.out
                    }
                    DefKind::Binding(pattern) => matches!(
                        self.ast.patterns[pattern],
                        Pattern::Binding { mutable: true, .. }
                    ),
                    _ => true,
                };
                (!mutable).then_some(Immutable::Variable(def))
            }
            // Through a reference, what counts is the reference: `&mut` allows it, even in a
            // variable that isn't `mut`.
            Expr::Field { .. } if self.is_prop(expr) => None,
            // What a `#T` shares is lent by its `get`: shared, never changed.
            Expr::Field { object, .. } | Expr::Index { object, .. }
                if self.shared.contains(object) =>
            {
                Some(Immutable::Shared)
            }
            Expr::Field { object, .. } | Expr::Index { object, .. } => {
                match self.reference(*object) {
                    Some(true) => None,
                    Some(false) => Some(Immutable::BehindSharedReference),
                    None => self.immutable(*object),
                }
            }
            Expr::Deref(inner) => match self.reference(*inner) {
                Some(false) => Some(Immutable::BehindSharedReference),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether `expr` is a `&mut` reference, a `&` one, or not a reference.
    fn reference(&self, expr: Id<Expr>) -> Option<bool> {
        let ty = self.exprs[expr.id]?;
        match self.u.get(self.u.shallow(ty)) {
            Ty::Ref { mutable, .. } => Some(*mutable),
            _ => None,
        }
    }

    /// Whether `expr` denotes a memory location that can be assigned to.
    fn is_place(&self, expr: Id<Expr>) -> bool {
        match &self.ast.exprs[expr] {
            Expr::Var(_) => self.names.expr(expr).is_none_or(|def| {
                matches!(
                    self.names.def(def).kind,
                    DefKind::Local(_)
                        | DefKind::Out(_)
                        | DefKind::Param { .. }
                        | DefKind::Binding(_)
                        // One that can't change, reported as that.
                        | DefKind::Const(_)
                )
            }),
            Expr::Field { object, .. } => !self.is_prop(expr) && self.is_place(*object),
            Expr::Index { object, .. } => self.is_place(*object),
            Expr::Deref(_) => true,
            _ => false,
        }
    }

    /// Whether `receiver_ty`, looking through a reference, could be what the impl block of
    /// method `func` is for, with `type_args` its instantiation. A `Vector<i32, 3>` isn't the
    /// `Vector<T, 1>` of `impl<T> Vector<T, 1>`: reported as that, before its `self` is
    /// compared and the difference gets lost in a mismatch about borrowing.
    fn on_impl_target(
        &mut self,
        func: Id<Func>,
        type_args: &[TypeId],
        receiver_ty: TypeId,
        name: &som_ast::Ident,
        what: &'static str,
    ) -> bool {
        if self.impl_target_fits(func, type_args, receiver_ty) {
            return true;
        }
        self.not_on_impl_target(&[func], receiver_ty, name, what);
        false
    }

    /// Reports that none of `funcs`, methods or properties called `name`, is in an impl block
    /// for `receiver_ty`, naming each block's target.
    fn not_on_impl_target(
        &mut self,
        funcs: &[Id<Func>],
        receiver_ty: TypeId,
        name: &som_ast::Ident,
        what: &'static str,
    ) {
        let mut found = self.u.shallow(receiver_ty);
        if let Ty::Ref { to, .. } = *self.u.get(found) {
            found = self.u.shallow(to);
        }
        let targets = (funcs.iter())
            .filter_map(|&func| {
                let index = self.names.impl_of(func)?;
                let target = self.impls[index].target?;
                Some((
                    self.u.display(target, self.names),
                    self.impl_target_span(index),
                ))
            })
            .collect();
        self.errors.push(TypeError::NotOnImplTarget {
            ty: self.describe(found),
            what,
            name: name.name.to_string(),
            targets,
            span: name.span.clone(),
        });
    }

    /// `values[0]`: the element of an array at an index it's known to have, a number written
    /// out that's less than its length. Anything that might be out of bounds is `get`'s.
    /// `values[i]`. An array's element, where `i` is known to be inside it: a number written
    /// out, or a variable below its length (`for i in 0..values.len`, `if i < values.len`). A
    /// slice's length is only known as the program runs, so a slice gives `&T?`: the element,
    /// or `none` past its end. Nothing is checked as it runs: what can't be known isn't allowed.
    fn index(&mut self, id: Id<Expr>, object: Id<Expr>, index: Id<Expr>) -> TypeId {
        let error = self.u.table.error;
        let object_ty = self.infer_object(object);
        // `xs[i]` of what implements `Index`: a `List`, `string`, a slice. What its `at`
        // lends, as a place; `at_mut`'s where it's changed (see `index_mut`). That `i` is
        // below its `len` is the prover's to check.
        let mut base = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = *self.u.get(base) {
            base = self.u.shallow(to);
        }
        let is_slice = matches!(self.u.get(base), Ty::Slice(_));
        if is_slice || matches!(self.u.get(base), Ty::Struct(..)) {
            if let Some(ty) = self.index_call(id, object, object_ty, base, index, "at") {
                return ty;
            }
            self.errors.push(TypeError::IndexNotArray {
                ty: self.describe(object_ty),
                span: self.span(object),
                get: self.get_suggestion(object, index),
            });
            self.infer(index);
            return error;
        }
        // A number written out is a position; a variable can be any integer type.
        let written = matches!(self.ast.exprs[index], Expr::Int(_));
        let index_ty = if written {
            self.u.table.usize
        } else {
            self.u.fresh(VarKind::Int)
        };
        self.expect(index, index_ty, None);
        // `[1, 2, 3][0]`: a collection nothing else decides is an array.
        if let Some(VarKind::Collection { element, len }) = self.u.var_kind(object_ty) {
            let array = self.u.intern(Ty::Array { element, len });
            let _ = self.u.unify(object_ty, array);
        }
        // Elements are reachable through one reference, as fields are.
        let mut viewed = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = *self.u.get(viewed) {
            viewed = self.u.shallow(to);
        }
        let (element, len) = match *self.u.get(viewed) {
            Ty::Array { element, len } => (element, len),
            Ty::Error => return error,
            _ => {
                self.errors.push(TypeError::IndexNotArray {
                    ty: self.describe(object_ty),
                    span: self.span(object),
                    get: self.get_suggestion(object, index),
                });
                return error;
            }
        };
        let len_ty = self.u.normalize(len);
        // Anything but a number written out: the prover checks it's below the length.
        let Expr::Int(at) = self.ast.exprs[index] else {
            return element;
        };
        match *self.u.get(self.u.shallow(len_ty)) {
            Ty::Value(Value::Int(n)) if (at as u128) < n => {}
            Ty::Value(Value::Int(n)) => self.errors.push(TypeError::IndexOutOfBounds {
                index: at as u128,
                len: n,
                span: self.span(index),
            }),
            Ty::Error => {}
            _ => self.errors.push(TypeError::IndexUnknownLength {
                len: self.u.display(len_ty, self.names),
                span: self.span(index),
                get: self.get_suggestion(object, index),
            }),
        }
        element
    }

    /// `object`, of type `ty`, read through to what it shares when it's a `#T` without a
    /// member called `name` of its own: `node.value` is `node.get().value`. Gives the type
    /// it's read as: the `&T` its `get` lends, which can't change what it refers to.
    fn through_shared(&mut self, object: Id<Expr>, ty: TypeId, name: &str) -> TypeId {
        let Some(rc) = self.names.lang_struct(som_resolve::LangStruct::Rc) else {
            return ty;
        };
        let Ty::Struct(def, ref args) = *self.u.get(self.u.shallow(ty)) else {
            return ty;
        };
        let [inner] = args[..] else {
            return ty;
        };
        let own = self.names.method(rc, name).is_some() || !self.names.props(rc, name).is_empty();
        if def != rc || own {
            return ty;
        }
        let lent = self.u.intern(Ty::Ref {
            mutable: false,
            to: inner,
        });
        self.shared.insert(object);
        lent
    }

    /// `values[i]` of type `base`: a call of `method`, `at` from its `impl Index` or
    /// `at_mut` from its `impl IndexMut`, giving what it lends. `None` if it has none.
    fn index_call(
        &mut self,
        id: Id<Expr>,
        object: Id<Expr>,
        object_ty: TypeId,
        base: TypeId,
        key: Id<Expr>,
        method: &str,
    ) -> Option<TypeId> {
        let lang = match method {
            "at" => LangTrait::Index,
            _ => LangTrait::IndexMut,
        };
        let index_trait = self.names.lang(lang)?;
        let (at, _) = {
            let u = &self.u;
            find_impl(&self.impls, &u.table, index_trait, base, &[], &|t| {
                u.shallow(t)
            })?
        };
        let func = (self.impls[at].methods.iter().copied())
            .find(|&m| &*self.ast.funcs[m].name.name == method)?;
        let (sig, type_args) = self.instantiate(func);
        self.methods.insert(id, self.names.func(func));
        self.require_bounds(func, &type_args, self.span(id));
        if !type_args.is_empty() {
            self.type_args.insert(id, type_args);
        }
        let (&self_ty, rest) = sig.params.split_first()?;
        if !self.receive(object, self_ty, object_ty) {
            self.mismatch(self_ty, object_ty, self.span(object), None);
        }
        let label = format!("{}::{method}", self.u.display(base, self.names));
        self.check_args_from(id, &label, func, 1, rest, &[key]);
        let ret = self.normalize_assoc(sig.ret);
        Some(match *self.u.get(self.u.shallow(ret)) {
            Ty::Ref { to, .. } => to,
            _ => ret,
        })
    }

    /// `xs[i]`, through `Index::at`, used where it's changed: `xs[i] = v`, `&mut xs[i]`,
    /// `xs[i].push(v)`. Becomes a call of `IndexMut::at_mut`, or, for a type without one,
    /// an error. False if `expr` isn't such an index.
    fn index_mut(&mut self, expr: Id<Expr>) -> bool {
        let Expr::Index { object, index } = self.ast.exprs[expr] else {
            return false;
        };
        let Some(&def) = self.methods.get(&expr) else {
            return false;
        };
        let DefKind::Func(func) = self.names.def(def).kind else {
            return false;
        };
        if &*self.ast.funcs[func].name.name != "at" {
            return true;
        }
        let Some(object_ty) = self.exprs[object.id] else {
            return true;
        };
        let mut base = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = *self.u.get(base) {
            base = self.u.shallow(to);
        }
        let read = self.exprs[expr.id];
        // How `at` took `xs` doesn't count: `at_mut` takes it its own way.
        self.receivers.remove(&object);
        let changed = self.index_call(expr, object, object_ty, base, index, "at_mut");
        match (changed, read) {
            (Some(changed), Some(read)) => {
                let _ = self.u.unify(changed, read);
            }
            _ => {
                let (object_text, index_text) =
                    (self.span(object).get_text(), self.span(index).get_text());
                self.errors.push(TypeError::SliceIndexWrite {
                    ty: self.describe(object_ty),
                    span: self.span(expr),
                    instead: format!("{object_text}.get_mut({index_text})"),
                });
            }
        }
        true
    }

    fn get_suggestion(&self, object: Id<Expr>, index: Id<Expr>) -> String {
        format!(
            "{}.get({})",
            self.span(object).get_text(),
            self.span(index).get_text()
        )
    }

    /// Whether `receiver_ty`, looking through a reference, could be what the impl block of
    /// method `func` is for, with `type_args` its instantiation. True when that's not known.
    /// The names of `owner`'s properties a value of type `ty` has: those of the impl blocks
    /// for its type, `magnitude` of a `Vector<3, f32>`.
    /// What a value of `ty`, of struct or built-in type `owner`, has that's seen from `at`:
    /// its fields, properties and methods, by name.
    fn members(
        &mut self,
        owner: DefId,
        ty: TypeId,
        at: &Span,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let viewer = self.viewer(at);
        let visible = |c: &Self, def: DefId| {
            visibility::hidden(c.ast, c.names, def, viewer.clone()).is_none()
        };
        let fields = match self.names.def(owner).kind {
            DefKind::Struct(s) => (self.ast.structs[s].fields.iter().enumerate())
                .filter(|&(i, _)| {
                    visibility::hidden_field(self.ast, self.names, s, i, viewer.clone()).is_none()
                })
                .map(|(_, f)| f.name.name.to_string())
                .collect(),
            _ => Vec::new(),
        };
        let props: Vec<String> = (self.props_for(owner, ty).into_iter())
            .filter(|name| (self.names.props(owner, name).iter()).any(|&d| visible(self, d)))
            .collect();
        // Only what takes `self` can be called on a value: not `string::from`. Nor what
        // the language calls itself: `drop`, `share`, and the operators' `add` and `eq`.
        let takes_self = |c: &Self, def: DefId| match c.names.def(def).kind {
            DefKind::Func(f) => {
                let implicit = (c.names.impl_of(f))
                    .and_then(|i| c.names.impls[i].trait_)
                    .and_then(|t| c.names.lang_of(t))
                    .is_some_and(|lang| {
                        matches!(lang, LangTrait::Drop | LangTrait::Share)
                            || lang.operator_method().is_some()
                    });
                !implicit
                    && (c.ast.funcs[f].params.first()).is_some_and(|p| &*p.name.name == "self")
            }
            _ => false,
        };
        let mut methods: Vec<String> = (self.names.methods_of(owner).into_iter())
            .filter(|&(_, def)| visible(self, def) && takes_self(self, def))
            .map(|(name, _)| name.to_string())
            .collect();
        methods.dedup();
        (fields, props, methods)
    }

    /// Whether `ty`, or what it refers to, is an array or slice, or a collection literal.
    fn is_collection(&self, ty: TypeId) -> bool {
        let mut ty = self.u.shallow(ty);
        if let Ty::Ref { to, .. } = *self.u.get(ty) {
            ty = self.u.shallow(to);
        }
        matches!(self.u.get(ty), Ty::Array { .. } | Ty::Slice(_))
            || matches!(self.u.var_kind(ty), Some(VarKind::Collection { .. }))
    }

    /// `x.name()`, the call `call` of `method` given nothing: from the name to the `)`,
    /// what a property goes in place of. `None` when it's given something.
    fn bare_call(
        &self,
        call: Id<Expr>,
        method: &som_ast::Ident,
        args: &[Id<Expr>],
    ) -> Option<Span> {
        if !args.is_empty() {
            return None;
        }
        let call = self.span(call);
        let end = call.start_offset + call.length;
        Some(Span {
            end: call.end,
            length: end - method.span.start_offset,
            ..method.span.clone()
        })
    }

    fn props_for(&mut self, owner: DefId, ty: TypeId) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for (name, def) in self.names.props_of(owner) {
            let DefKind::Func(func) = self.names.def(def).kind else {
                continue;
            };
            if names.last().is_some_and(|last| last == name) {
                continue;
            }
            let (_, type_args) = self.instantiate(func);
            if self.impl_target_fits(func, &type_args, ty) {
                names.push(name.to_string());
            }
        }
        names
    }

    fn impl_target_fits(
        &mut self,
        func: Id<Func>,
        type_args: &[TypeId],
        receiver_ty: TypeId,
    ) -> bool {
        let Some(index) = self.names.impl_of(func) else {
            return true;
        };
        let Some(target) = self.impls[index].target else {
            return true;
        };
        let params = self.impls[index].params.clone();
        let Some(args) = type_args.get(..params.len()) else {
            return true;
        };
        let wanted = crate::traits::substitute(&mut self.u.table, target, &params, args);
        let wanted = self.u.normalize(wanted);
        // Through a reference on either side: a `str` is one already.
        let unref = |u: &Unifier, ty: TypeId| match *u.get(u.shallow(ty)) {
            Ty::Ref { to, .. } => u.shallow(to),
            _ => u.shallow(ty),
        };
        let found = unref(&self.u, receiver_ty);
        self.u.could_fit(unref(&self.u, wanted), found)
    }

    /// Makes `ty`, text in quotes nothing's decided yet, the standard library's `string`,
    /// whose methods and fields it then has. Without it loaded, text stays undecided.
    fn text_as_string(&mut self, ty: TypeId) {
        let text = self.u.table.text;
        if self.u.var_kind(ty) == Some(VarKind::Text) && text != self.u.table.str_ref {
            let _ = self.u.unify(ty, text);
        }
    }

    /// Settles the numbers in `ty` that nothing has decided yet to what they'd end up as
    /// anyway, `i32` and `f64`. For choosing between impl blocks that an undecided
    /// `[1.0, 2.0]` would all fit: the choice is the one it gets with nothing else said,
    /// not whichever block came first. Returns whether anything was settled.
    fn default_numbers(&mut self, ty: TypeId) -> bool {
        let ty = self.u.shallow(ty);
        match self.u.var_kind(ty) {
            Some(VarKind::Int) => {
                let int = self.u.table.i32;
                return self.u.unify(ty, int).is_ok();
            }
            Some(VarKind::Float) => {
                let float = self.u.table.f64;
                return self.u.unify(ty, float).is_ok();
            }
            Some(VarKind::Collection { element, len }) => {
                let settled = self.default_numbers(element);
                return self.default_numbers(len) || settled;
            }
            Some(_) => return false,
            None => {}
        }
        let children = self.u.get(ty).children();
        let mut settled = false;
        for child in children {
            settled |= self.default_numbers(child);
        }
        settled
    }

    /// Whether `expr` reads a property, `list.len`, rather than a field. Known once it's
    /// inferred.
    fn is_prop(&self, expr: Id<Expr>) -> bool {
        matches!(self.ast.exprs[expr], Expr::Field { .. }) && self.methods.contains_key(&expr)
    }

    /// `list.len` of a property, `prop len: usize`: a call of it, without
    /// parentheses. `None` if `object` has no property `field`; fields are named apart.
    fn prop_access(
        &mut self,
        id: Id<Expr>,
        object: Id<Expr>,
        object_ty: TypeId,
        field: &som_ast::Ident,
    ) -> Option<TypeId> {
        if let Some(ty) = self.bound_prop(id, object, object_ty, field) {
            return Some(ty);
        }
        let owner = self.primitive_owner(object_ty).or_else(|| {
            let mut ty = self.u.shallow(object_ty);
            if let Ty::Ref { to, .. } = self.u.get(ty) {
                ty = self.u.shallow(*to);
            }
            match self.u.get(ty) {
                Ty::Struct(def, _) => Some(*def),
                _ => None,
            }
        })?;
        let candidates = self.names.props(owner, &field.name);
        if candidates.is_empty() {
            return None;
        }
        let func_of = |c: &Self, def: DefId| match c.names.def(def).kind {
            DefKind::Func(func) => func,
            _ => unreachable!("properties are functions"),
        };
        // Text written in quotes is a `string` here.
        if self.u.var_kind(object_ty) == Some(VarKind::Text) {
            let _ = self.u.unify(object_ty, self.u.table.text);
        }
        // Of the impl blocks with one, the one for the value's type: `x` of a `Vector<3, T>`.
        // If several fit a value with numbers nothing has decided, `[1.0, 2.0]`, they're
        // settled to `f64` and `i32` first, as they would be anyway.
        let fitting = |c: &mut Self| {
            let mut fitting = Vec::new();
            for &def in &candidates {
                let func = func_of(c, def);
                let (sig, type_args) = c.instantiate(func);
                if c.impl_target_fits(func, &type_args, object_ty) {
                    fitting.push((def, func, sig, type_args));
                }
            }
            fitting
        };
        let mut found = fitting(self);
        if found.len() > 1 && self.default_numbers(object_ty) {
            found = fitting(self);
        }
        let chosen = found.into_iter().next();
        let Some((def, func, sig, type_args)) = chosen else {
            let funcs: Vec<Id<Func>> = candidates.iter().map(|&d| func_of(self, d)).collect();
            self.not_on_impl_target(&funcs, object_ty, field, "property");
            return Some(self.u.table.error);
        };
        self.methods.insert(id, def);
        self.check_visible(def, &field.span);
        self.require_bounds(func, &type_args, self.span(id));
        if !type_args.is_empty() {
            self.type_args.insert(id, type_args);
        }
        // `self` is its only parameter; the resolver reports it if that's not so.
        if let Some(&self_ty) = sig.params.first()
            && !self.receive(object, self_ty, object_ty)
        {
            let from = (self.ast.funcs[func].params[0].ty).map(|t| self.ast.types.span(t).clone());
            self.mismatch(self_ty, object_ty, self.span(object), from);
        }
        Some(sig.ret)
    }

    /// `x.len` of an `x` whose type is a type parameter, or `Self` in a trait: the property
    /// of a trait it has to implement, or one that trait builds on.
    fn bound_prop(
        &mut self,
        id: Id<Expr>,
        object: Id<Expr>,
        object_ty: TypeId,
        field: &som_ast::Ident,
    ) -> Option<TypeId> {
        let mut base = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = *self.u.get(base) {
            base = self.u.shallow(to);
        }
        let Ty::Param(param) = *self.u.get(base) else {
            return None;
        };
        let mut traits = self.names.bounds(param).to_vec();
        let mut i = 0;
        while i < traits.len() {
            if let DefKind::Trait(t) = self.names.def(traits[i]).kind {
                for &s in self.names.supers(t) {
                    if !traits.contains(&s) {
                        traits.push(s);
                    }
                }
            }
            i += 1;
        }
        let func = traits.iter().find_map(|&t| {
            let DefKind::Trait(t) = self.names.def(t).kind else {
                return None;
            };
            (self.ast.traits[t].methods.iter().copied())
                .find(|&m| self.ast.funcs[m].prop && self.ast.funcs[m].name.name == field.name)
        })?;
        let (sig, type_args) = self.instantiate(func);
        let _ = self.u.unify(type_args[0], base);
        self.methods.insert(id, self.names.func(func));
        self.type_args.insert(id, type_args);
        if let Some(&self_ty) = sig.params.first()
            && !self.receive(object, self_ty, object_ty)
        {
            self.mismatch(self_ty, object_ty, self.span(object), None);
        }
        Some(self.normalize_assoc(sig.ret))
    }

    // --- Operators ---

    fn binary(&mut self, id: Id<Expr>, op: BinOp, lhs: TypeId, rhs: TypeId) -> TypeId {
        let table = &self.u.table;
        let (error, bool, pointer) = (table.error, table.bool, table.pointer);

        // Pointer arithmetic: `ptr + n`.
        if op == BinOp::Add && self.u.shallow(lhs) == pointer {
            let int = self.u.fresh(VarKind::Int);
            if self.u.unify(int, rhs).is_err() {
                self.invalid_operands(id, op, lhs, rhs);
            }
            return pointer;
        }

        if matches!(op, BinOp::And | BinOp::Or) {
            if self.u.unify(lhs, bool).is_err() || self.u.unify(rhs, bool).is_err() {
                self.invalid_operands(id, op, lhs, rhs);
            }
            return bool;
        }

        // `a << n`: an integer `a`, moved by any integer `n`.
        if matches!(op, BinOp::ShiftLeft | BinOp::ShiftRight) {
            let amount = self.u.fresh(VarKind::Int);
            if self.u.unify(amount, rhs).is_err() {
                self.invalid_operands(id, op, lhs, rhs);
                return error;
            }
            match self.operand_ok(op, lhs) {
                Some(true) => {}
                Some(false) => self.invalid_operands(id, op, lhs, rhs),
                None => self.deferred_ops.push((id, op, lhs, lhs)),
            }
            return lhs;
        }

        // `"ab" * 3`, `money + money`: an implementation of the operator's trait for these
        // operands, which says what comes out.
        if let Some(lang) = LangTrait::of_operator(op)
            && let Some(output) = self.operator_impl(id, lang, lhs, rhs)
        {
            return output;
        }

        // `"score: " + n`: an operand of another type than one whose operator is a trait's
        // method becomes that type, through `From`, as an argument would.
        let (mut lhs, mut rhs) = (lhs, rhs);
        if let Expr::Binary {
            lhs: left,
            rhs: right,
            ..
        } = self.ast.exprs[id]
            && let Some(lang) = LangTrait::of_operator(op)
            && !self.u.could_fit(lhs, rhs)
        {
            let has_op = |c: &mut Self, ty: TypeId| {
                let ty = c.u.shallow(ty);
                matches!(c.u.get(ty), Ty::Struct(..)) && c.self_operator(ty, lang).is_some()
            };
            // As the operand is, before being read through: `r` of a `&i32` as a `&i32`.
            let own = |c: &Self, side: Id<Expr>, ty: TypeId| c.exprs[side.id].unwrap_or(ty);
            let (left_own, right_own) = (own(self, left, lhs), own(self, right, rhs));
            if has_op(self, lhs) && self.from_conversion(right, lhs, right_own) {
                rhs = lhs;
            } else if has_op(self, rhs) && self.from_conversion(left, rhs, left_own) {
                lhs = rhs;
            }
        }
        let result = if is_arithmetic(op) { lhs } else { bool };
        if self.u.unify(lhs, rhs).is_err() {
            self.invalid_operands(id, op, lhs, rhs);
            return if is_arithmetic(op) { error } else { bool };
        }
        // Values of a type implementing `Eq` compare with its `eq`, `Add` add with its `add`.
        if let Some(output) = self.trait_op(id, op, lhs) {
            return if is_arithmetic(op) { output } else { bool };
        }
        match self.operand_ok(op, lhs) {
            Some(true) => {}
            Some(false) => self.invalid_operands(id, op, lhs, rhs),
            None => self.deferred_ops.push((id, op, lhs, rhs)),
        }
        result
    }

    /// An operand of an arithmetic or comparison operator, of type `ty`: a reference to a
    /// number, `bool` or struct is read through, so `self * self` works for a `&i32`, and
    /// `"arg: " + arg` for a `&string`.
    fn through_reference(&mut self, operand: Id<Expr>, ty: TypeId) -> TypeId {
        let Ty::Ref { to, .. } = *self.u.get(self.u.shallow(ty)) else {
            return ty;
        };
        let to = self.u.shallow(to);
        let readable = matches!(self.u.get(to), Ty::Int(_) | Ty::Float(_) | Ty::Bool)
            || matches!(self.u.var_kind(to), Some(VarKind::Int | VarKind::Float))
            || matches!(self.u.get(to), Ty::Struct(..));
        if !readable {
            return ty;
        }
        self.read_through.insert(operand);
        to
    }

    /// Whether `ty` is an enum whose variants hold nothing: a number, really, which can be
    /// compared and converted to one.
    fn is_plain_enum(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Struct(def, _) => match self.names.def(*def).kind {
                DefKind::Struct(s) => {
                    let s = &self.ast.structs[s];
                    s.is_enum() && s.payload().next().is_none()
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Whether `op` is defined on operands of type `ty`, or `None` if `ty` isn't known yet.
    fn operand_ok(&self, op: BinOp, ty: TypeId) -> Option<bool> {
        if let Some(kind) = self.u.var_kind(ty) {
            return match kind {
                VarKind::Any => None,
                VarKind::Int => Some(true),
                VarKind::Float => Some(!is_bitwise(op)),
                VarKind::Collection { .. } | VarKind::Text => Some(false),
            };
        }
        let ty = self.u.get(self.u.shallow(ty));
        Some(match op {
            _ if matches!(ty, Ty::Error) => true,
            BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide | BinOp::Modulo => {
                matches!(ty, Ty::Int(_) | Ty::Float(_))
            }
            BinOp::LessThan
            | BinOp::GreaterThan
            | BinOp::LessThanOrEqual
            | BinOp::GreaterThanOrEqual => {
                matches!(ty, Ty::Int(_) | Ty::Float(_) | Ty::Char)
            }
            BinOp::Equals | BinOp::NotEquals => {
                matches!(
                    ty,
                    Ty::Int(_) | Ty::Float(_) | Ty::Char | Ty::Bool | Ty::Pointer
                ) || self.is_plain_enum(ty)
            }
            BinOp::And | BinOp::Or => matches!(ty, Ty::Bool),
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
                matches!(ty, Ty::Int(_))
            }
        })
    }
}

/// Whether `op` gives a value of its operands' type, rather than a `bool`.
fn is_arithmetic(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide | BinOp::Modulo
    ) || is_bitwise(op)
}

/// Whether `op` works on an integer's bits: `&`, `|`, `^`, `<<`, `>>`.
fn is_bitwise(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight
    )
}

/// A call more than one function fitted when it was checked: see `resolve_pending_calls`.
struct PendingCall {
    call: Id<Expr>,
    name: String,
    args: Vec<Id<Expr>>,
    /// The arguments' types.
    found: Vec<TypeId>,
    /// The functions that still fit: each one's definition, function, signature (its type
    /// parameters fresh variables) and those variables.
    fitting: Vec<(DefId, Id<Func>, Signature, Vec<TypeId>)>,
    /// The call's type, until it's decided.
    result: TypeId,
}

/// What a condition says about optional places when it's true, and when it's false.
#[derive(Default)]
struct Facts {
    when_true: Vec<Narrowing>,
    when_false: Vec<Narrowing>,
}

/// Why a place can't be changed.
enum Immutable {
    /// A variable declared without `mut`.
    Variable(DefId),
    /// A constant: the same everywhere, never changed.
    Const(DefId),
    BehindSharedReference,
    /// What a `#T` shares: every copy sees it, so nothing changes it.
    Shared,
}

// --- Narrowing ---
//
// Inside `if x { … }` (or `if x != none`), where `x` is an optional variable or field, `x`
// means the value it holds. Anything that could make `x` `none` again ends that: assigning
// it or what contains it, `&mut` of what contains it, or passing on a `&mut` it's behind.
// A loop is checked for those first, since a later iteration sees changes further down.

/// An optional place known to hold a value.
#[derive(Clone)]
struct Narrowing {
    id: usize,
    key: Key,
    inner: TypeId,
}

/// A place as written: a variable, then fields and dereferences.
#[derive(Clone, PartialEq)]
struct Key {
    root: DefId,
    path: Vec<Step>,
}

#[derive(Clone, PartialEq)]
enum Step {
    Field(Box<str>),
    Deref,
}

impl Key {
    /// The fields, without the dereferences between them: `p.x` and `(*p).x` are the same
    /// memory, since a field is reached through a reference without writing the `*`.
    fn fields(&self) -> impl Iterator<Item = &Step> {
        self.path.iter().filter(|s| matches!(s, Step::Field(_)))
    }

    /// How many dereferences end the path: `*r` is what `r` points to, not `r` itself.
    fn trailing_derefs(&self) -> usize {
        self.path
            .iter()
            .rev()
            .take_while(|s| **s == Step::Deref)
            .count()
    }

    /// Whether this place is `other`, or inside it.
    fn within(&self, other: &Key) -> bool {
        let mut mine = self.fields();
        let prefix =
            self.root == other.root && other.fields().all(|step| mine.next() == Some(step));
        // Past `other`'s fields, it's inside; at the same fields, it's `other` itself or what
        // `other` points to, not a reference `other` is behind.
        prefix && (mine.next().is_some() || other.trailing_derefs() <= self.trailing_derefs())
    }

    fn same(&self, other: &Key) -> bool {
        self.within(other) && other.within(self)
    }
}

impl Checker<'_> {
    /// The place `expr` denotes, if it's a variable, field or dereference of one.
    fn key(&self, expr: Id<Expr>) -> Option<Key> {
        match &self.ast.exprs[expr] {
            Expr::Var(_) => {
                let def = self.names.expr(expr)?;
                matches!(
                    self.names.def(def).kind,
                    DefKind::Local(_)
                        | DefKind::Param { .. }
                        | DefKind::Out(_)
                        | DefKind::Binding(_)
                )
                .then(|| Key {
                    root: def,
                    path: Vec::new(),
                })
            }
            Expr::Field { .. } if self.is_prop(expr) => None,
            Expr::Field { object, field } => {
                let mut key = self.key(*object)?;
                key.path.push(Step::Field(field.name.clone()));
                Some(key)
            }
            Expr::Deref(inner) => {
                let mut key = self.key(*inner)?;
                key.path.push(Step::Deref);
                Some(key)
            }
            _ => None,
        }
    }

    /// The type `expr` holds, if it's a narrowed place. Only the same spelling matches, so
    /// `(*p).x` isn't narrowed by `if p.x`, which is safe, just less convenient.
    fn narrowed_type(&self, expr: Id<Expr>) -> Option<TypeId> {
        if self.narrowed.is_empty() {
            return None;
        }
        let key = self.key(expr)?;
        let found = self.narrowed.iter().rev().find(|n| n.key == key)?;
        Some(found.inner)
    }

    /// Ends the narrowings of the places `changed` says could change.
    fn forget(&mut self, changed: impl Fn(&Key) -> bool) {
        self.narrowed.retain(|n| !changed(&n.key));
    }

    fn is_mut_ref(&self, ty: TypeId) -> bool {
        matches!(
            self.u.get(self.u.shallow(ty)),
            Ty::Ref { mutable: true, .. }
        )
    }

    /// Ends the narrowings of the places `stmts` could change.
    fn forget_changed_in(&mut self, stmts: &[Id<Stmt>]) {
        if self.narrowed.is_empty() {
            return;
        }
        for &stmt in stmts {
            match &self.ast.stmts[stmt] {
                Stmt::Let { value, .. } => self.forget_changed_by(*value, false),
                Stmt::Expr(expr) => self.forget_changed_by(*expr, false),
                Stmt::Loop { body } => self.forget_changed_in(body),
                Stmt::For { iterable, body, .. } => {
                    self.forget_changed_by(*iterable, false);
                    self.forget_changed_in(body);
                }
                Stmt::While { condition, body } => {
                    self.forget_changed_by(*condition, false);
                    self.forget_changed_in(body);
                }
                Stmt::Break(_) | Stmt::Continue(_) | Stmt::Use(_) | Stmt::Const(_) => {}
                Stmt::Return(value) => {
                    if let Some(value) = value {
                        self.forget_changed_by(*value, false);
                    }
                }
                Stmt::If {
                    condition,
                    then,
                    otherwise,
                } => {
                    self.forget_changed_by(*condition, false);
                    self.forget_changed_in(then);
                    if let Some(otherwise) = otherwise {
                        self.forget_changed_in(otherwise);
                    }
                }
            }
        }
    }

    /// Ends the narrowings of the places `expr` could change. `in_place`: it's the object of
    /// a field access or dereference.
    fn forget_changed_by(&mut self, expr: Id<Expr>, in_place: bool) {
        match &self.ast.exprs[expr] {
            Expr::Interpolated(parts) => {
                for &part in parts {
                    if let som_ast::Part::Value(value) = part {
                        self.forget_changed_by(value, false);
                    }
                }
            }
            Expr::Fail(message) => self.forget_changed_by(*message, false),
            // Its value changes nothing the function has.
            Expr::InPlace(_)
            | Expr::Hole
            | Expr::Unreachable
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None
            | Expr::Out(_) => {}
            Expr::Var(_) => {
                if !in_place
                    && let Some(def) = self.names.expr(expr)
                    && let Some(&ty) = self.locals.get(&def)
                    && self.is_mut_ref(ty)
                {
                    self.forget(|key| key.root == def && !key.path.is_empty());
                }
            }
            Expr::Field { object, .. } | Expr::Deref(object) => {
                self.forget_changed_by(*object, true)
            }
            Expr::Index { object, index } => {
                self.forget_changed_by(*object, true);
                self.forget_changed_by(*index, false);
            }
            Expr::Borrow { mutable, expr } => {
                if *mutable && let Some(borrowed) = self.key(*expr) {
                    self.forget(|key| key.within(&borrowed) && !key.same(&borrowed));
                }
                self.forget_changed_by(*expr, false);
            }
            Expr::Assign { target, value } => {
                if let Some(assigned) = self.key(*target) {
                    self.forget(|key| key.within(&assigned));
                }
                self.forget_changed_by(*target, true);
                self.forget_changed_by(*value, false);
            }
            Expr::Not(e)
            | Expr::Neg(e)
            | Expr::BitNot(e)
            | Expr::ArrayRepeat { value: e, .. }
            | Expr::Cast { value: e, .. } => self.forget_changed_by(*e, false),
            Expr::Range { start, end } => {
                for &bound in start.iter().chain(end) {
                    self.forget_changed_by(bound, false);
                }
            }
            Expr::Array(items) => {
                for &item in items {
                    self.forget_changed_by(item, false);
                }
            }
            Expr::Binary { lhs, rhs, .. }
            | Expr::Fallback {
                optional: lhs,
                fallback: rhs,
            } => {
                self.forget_changed_by(*lhs, false);
                self.forget_changed_by(*rhs, false);
            }
            Expr::Call { args, .. } => {
                for &arg in args {
                    self.forget_changed_by(arg, false);
                }
            }
            Expr::MethodCall { receiver, args, .. } => {
                self.forget_changed_by(*receiver, false);
                for &arg in args {
                    self.forget_changed_by(arg, false);
                }
            }
            Expr::Construct { fields, .. } => {
                for &(_, value) in fields {
                    self.forget_changed_by(value, false);
                }
            }
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                for e in [*condition, *then, *otherwise] {
                    self.forget_changed_by(e, false);
                }
            }
            Expr::Block { stmts, value } => {
                self.forget_changed_in(stmts);
                if let Some(value) = value {
                    self.forget_changed_by(*value, false);
                }
            }
            Expr::Match { scrutinee, arms } => {
                self.forget_changed_by(*scrutinee, false);
                for arm in arms {
                    self.forget_changed_by(arm.body, false);
                }
            }
        }
    }
}

/// Whether the code at `span` stands alone between its neighbours: a `let`'s value, an
/// argument, what's returned. Anything put in its place needs no parentheses there.
fn stands_alone(span: &Span) -> bool {
    let source = span.source.content();
    let before = source[..span.start_offset].trim_end();
    let after = source[(span.start_offset + span.length).min(source.len())..].trim_start();
    let opens = match before.chars().last() {
        Some('(' | ',' | '{') => true,
        // `=`, but not `==`, `!=`, `<=` or `>=`.
        Some('=') => !before[..before.len() - 1].ends_with(['=', '!', '<', '>']),
        _ => before.ends_with("return"),
    };
    opens && after.starts_with([';', ',', ')', '}'])
}
