use som_ast::{Ast, BinOp, Expr, Func, Id, Primitive, Span, Stmt, TypeExpr};
use som_resolve::{DefId, DefKind, Names};
use std::collections::{HashMap, HashSet};

use crate::error::TypeError;
use crate::ty::{IntTy, Ty, TypeId};
use crate::unify::{Unifier, VarKind};
use crate::{ArrayMethod, Loop, Signature, Typed};
use som_base::Message;
use som_base::message;

pub(crate) struct Checker<'a> {
    ast: &'a Ast,
    names: &'a Names,
    u: Unifier,
    errors: Vec<TypeError>,
    exprs: Vec<Option<TypeId>>,
    locals: HashMap<DefId, TypeId>,
    sigs: Vec<Signature>,
    structs: Vec<Vec<TypeId>>,
    methods: HashMap<Id<Expr>, DefId>,
    fields: HashMap<Id<Expr>, usize>,
    type_args: HashMap<Id<Expr>, Vec<TypeId>>,
    wrapped: HashSet<Id<Expr>>,
    /// The optional places known to hold a value here, innermost last.
    narrowed: Vec<Narrowing>,
    next_narrowing: usize,
    /// Reads of narrowed places: they read the value, not the optional.
    narrowed_reads: HashSet<Id<Expr>>,
    array_methods: HashMap<Id<Expr>, ArrayMethod>,
    sliced: HashMap<Id<Expr>, TypeId>,
    for_loops: HashMap<Id<Stmt>, Loop>,
    /// The next expression inferred is the object of a field access or dereference.
    in_place: bool,
    /// Operators whose operand type wasn't known yet when they were checked.
    deferred_ops: Vec<(Id<Expr>, BinOp, TypeId, TypeId)>,
    /// How many `loop`s and `while`s enclose the statement being checked.
    loops: usize,
}

impl<'a> Checker<'a> {
    pub fn new(ast: &'a Ast, names: &'a Names) -> Self {
        Checker {
            ast,
            names,
            u: Unifier::new(),
            errors: Vec::new(),
            exprs: vec![None; ast.exprs.len()],
            locals: HashMap::new(),
            sigs: Vec::new(),
            structs: Vec::new(),
            methods: HashMap::new(),
            fields: HashMap::new(),
            type_args: HashMap::new(),
            wrapped: HashSet::new(),
            narrowed: Vec::new(),
            next_narrowing: 0,
            narrowed_reads: HashSet::new(),
            array_methods: HashMap::new(),
            sliced: HashMap::new(),
            for_loops: HashMap::new(),
            in_place: false,
            deferred_ops: Vec::new(),
            loops: 0,
        }
    }

    pub fn run(mut self) -> (Typed, Vec<TypeError>) {
        let ast = self.ast;

        // Struct fields and function signatures first, so bodies can use them in any order.
        for (_, s) in ast.structs.iter() {
            let fields = s.fields.iter().map(|f| self.lower(f.ty)).collect();
            self.structs.push(fields);
        }
        for (id, func) in ast.funcs.iter() {
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
                (Some(ty), _) => self.lower(ty),
                (None, None) => self.u.table.unit,
                (None, Some(_)) => self.u.fresh(VarKind::Any),
            };
            self.sigs.push(Signature { params, ret });
            for (i, &def) in self.names.params(id).iter().enumerate() {
                self.locals.insert(def, self.sigs[id.id].params[i]);
            }
        }

        for (id, func) in ast.funcs.iter() {
            if let Some(body) = func.body {
                self.check_body(id, func, body);
            }
        }

        for (expr, op, lhs, rhs) in std::mem::take(&mut self.deferred_ops) {
            if self.operand_ok(op, lhs) == Some(false) || self.u.var_kind(lhs) == Some(VarKind::Any)
            {
                self.invalid_operands(expr, op, lhs, rhs);
            }
        }

        self.finish()
    }

    fn check_body(&mut self, id: Id<Func>, func: &Func, body: Id<Expr>) {
        let ret = self.sigs[id.id].ret;
        let body_ty = self.infer(body);
        if !self.coerce(body, ret, body_ty) {
            // Point at the value the body produces, not the whole block.
            let at = match &self.ast.exprs[body] {
                Expr::Block {
                    value: Some(value), ..
                } => *value,
                _ => body,
            };
            let from = func.returns.map(|t| self.ast.types.span(t).clone());
            self.mismatch(ret, body_ty, self.ast.exprs.span(at).clone(), from);
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
            let max: u128 = match int {
                IntTy::I8 => i8::MAX as u128,
                IntTy::I16 => i16::MAX as u128,
                IntTy::I32 => i32::MAX as u128,
                IntTy::I64 | IntTy::ISize => i64::MAX as u128,
                IntTy::U8 => u8::MAX as u128,
                IntTy::U16 => u16::MAX as u128,
                IntTy::U32 => u32::MAX as u128,
                IntTy::U64 | IntTy::USize | IntTy::I128 | IntTy::U128 => continue,
            };
            if *n as u128 > max {
                self.errors.push(TypeError::LiteralOutOfRange {
                    value: *n,
                    ty: format!("{int:?}").to_lowercase(),
                    max,
                    span: self.span(id),
                });
            }
        }
    }

    /// Replaces all inference variables, producing the final tables.
    fn finish(mut self) -> (Typed, Vec<TypeError>) {
        let mut reported = HashSet::new();
        let error = self.u.table.error;

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
            methods: self.methods,
            fields: self.fields,
            type_args,
            wrapped: self.wrapped,
            narrowed_reads: self.narrowed_reads,
            array_methods: self.array_methods,
            sliced,
            loops: self.for_loops,
        };
        (typed, self.errors)
    }

    // --- Types ---

    /// The type a type annotation describes.
    fn lower(&mut self, id: Id<TypeExpr>) -> TypeId {
        let ty = match &self.ast.types[id] {
            TypeExpr::Unit => Ty::Unit,
            // Text is a view of bytes, like a slice: `str` is a reference to them.
            TypeExpr::Primitive(Primitive::Str) => return self.u.table.str_ref,
            TypeExpr::Primitive(p) => Ty::from_primitive(*p),
            TypeExpr::Pointer => Ty::Pointer,
            TypeExpr::Named(_) => match self
                .names
                .ty(id)
                .map(|def| (def, &self.names.def(def).kind))
            {
                Some((def, DefKind::Struct(_))) => Ty::Struct(def),
                Some((def, DefKind::TypeParam { .. })) => Ty::Param(def),
                _ => Ty::Error, // already reported by the resolver
            },
            // `&str` from before strings were slices.
            TypeExpr::Reference { to, .. }
                if matches!(self.ast.types[*to], TypeExpr::Primitive(Primitive::Str)) =>
            {
                self.errors.push(TypeError::StrBehindReference {
                    span: self.ast.types.span(id).clone(),
                });
                return self.u.table.str_ref;
            }
            TypeExpr::Reference { mutable, to, .. } => {
                let to = self.lower(*to);
                Ty::Ref {
                    mutable: *mutable,
                    to,
                }
            }
            TypeExpr::Optional(inner) => Ty::Optional(self.lower(*inner)),
            TypeExpr::Array { element, len } => Ty::Array {
                element: self.lower(*element),
                len: *len,
            },
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

    /// A function's signature with its type parameters replaced by fresh variables.
    fn instantiate(&mut self, func: Id<Func>) -> (Signature, Vec<TypeId>) {
        let sig = self.sigs[func.id].clone();
        let type_params = self.names.type_params(func);
        if type_params.is_empty() {
            return (sig, Vec::new());
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

    fn substitute(&mut self, ty: TypeId, subst: &[(DefId, TypeId)]) -> TypeId {
        match self.u.get(ty).clone() {
            Ty::Param(def) => subst
                .iter()
                .find(|(d, _)| *d == def)
                .map_or(ty, |(_, t)| *t),
            other if other.children().is_empty() => ty,
            other => {
                let substituted = other.map_children(|child| self.substitute(child, subst));
                self.u.intern(substituted)
            }
        }
    }

    // --- Errors ---

    /// A type for an error message. A type that isn't known yet can't be written
    /// down, so it's described in words instead ("an integer"), still shown as a type.
    fn describe(&self, ty: TypeId) -> Message {
        let text = match self.u.var_kind(ty) {
            Some(VarKind::Int) => "an integer".to_string(),
            Some(VarKind::Float) => "a float".to_string(),
            Some(VarKind::Text) => "a string".to_string(),
            Some(VarKind::Collection { len: 1, .. }) => "a collection of 1 element".to_string(),
            Some(VarKind::Collection { len, .. }) => format!("a collection of {len} elements"),
            Some(VarKind::Any) => "an unknown type".to_string(),
            None => self.u.display(ty, self.names),
        };
        message::ty(text).into()
    }

    fn mismatch(
        &mut self,
        expected: TypeId,
        found: TypeId,
        span: Span,
        expected_from: Option<Span>,
    ) {
        self.errors.push(TypeError::Mismatch {
            expected: self.describe(expected),
            found: self.describe(found),
            span,
            expected_from,
        });
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
        self.errors.push(TypeError::InvalidOperands {
            op: op.symbol().into(),
            lhs: self.describe(lhs),
            rhs: self.describe(rhs),
            span: self.ast.exprs.span(expr).clone(),
            optional,
        });
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

    // --- Statements ---

    fn check_stmts(&mut self, stmts: &[Id<Stmt>]) {
        let outer: HashSet<usize> = self.narrowed.iter().map(|n| n.id).collect();
        for &stmt in stmts {
            self.check_stmt(stmt);
        }
        // What an assignment in here narrowed ends with the block; what ended stays ended.
        self.narrowed.retain(|n| outer.contains(&n.id));
    }

    fn check_loop(&mut self, body: &[Id<Stmt>]) {
        // A later iteration sees what the body changes, even further down.
        self.forget_changed_in(body);
        self.loops += 1;
        self.check_stmts(body);
        self.loops -= 1;
    }

    fn check_stmt(&mut self, id: Id<Stmt>) {
        match &self.ast.stmts[id] {
            Stmt::Let { ty, value, .. } => {
                let ty = match ty {
                    Some(annotation) => {
                        let expected = self.lower(*annotation);
                        let from = self.ast.types.span(*annotation).clone();
                        self.expect(*value, expected, Some(from));
                        expected
                    }
                    None => self.infer(*value),
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
                self.infer(*expr);
            }
            Stmt::Loop { body } => self.check_loop(body),
            Stmt::While { condition, body } => {
                // The condition runs again after the body.
                self.forget_changed_in(body);
                let bool = self.u.table.bool;
                self.expect(*condition, bool, None);
                self.check_loop(body);
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
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let facts = self.check_condition(*condition);
                self.narrowed_while(facts.when_true, |c| c.check_stmts(then));
                if let Some(otherwise) = otherwise {
                    self.narrowed_while(facts.when_false, |c| c.check_stmts(otherwise));
                }
            }
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
        let viewed = match *self.u.get(shallow) {
            Ty::Ref { to, .. } => self.u.shallow(to),
            _ => shallow,
        };
        let element = match *self.u.get(shallow) {
            Ty::Range(int) => {
                self.for_loops.insert(stmt, Loop::Range);
                return int;
            }
            Ty::Error => return error,
            _ => match *self.u.get(viewed) {
                Ty::Array { element, .. } | Ty::Slice(element) => element,
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
        // Elements that move are lent, not moved out.
        let by_reference = !self.is_copy(element);
        self.for_loops.insert(stmt, Loop::Elements { by_reference });
        if by_reference {
            self.u.intern(Ty::Ref {
                mutable: false,
                to: element,
            })
        } else {
            element
        }
    }

    /// Checks a condition: a `bool`, or an optional, which is true when it holds a value
    /// (`if x` is `if x != none`), combined with `and`, `or` and `!`. Returns what it being
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

    /// Checks a condition that isn't `and`, `or` or `!`.
    fn check_condition_operand(&mut self, condition: Id<Expr>) -> Facts {
        let mut facts = Facts::default();
        match self.condition_narrowing(condition) {
            Some((narrowing, true)) => facts.when_true.push(narrowing),
            Some((narrowing, false)) => facts.when_false.push(narrowing),
            None => {}
        }
        facts
    }

    /// Checks a condition that isn't `and`, `or` or `!`, returning the optional place it tells
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

    /// Runs `f` with `narrowing` in effect, if there is one.
    fn narrowed_while<T>(
        &mut self,
        narrowings: Vec<Narrowing>,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.narrowed_during(narrowings, f).0
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
        let found = self.infer(expr);
        if self.coerce(expr, expected, found) {
            return;
        }
        // A `str` where C needs a `cstr`: say why they differ.
        if self.u.shallow(expected) == self.u.table.cstr
            && self.u.shallow(found) == self.u.table.str_ref
        {
            self.errors.push(TypeError::StrToC {
                span: self.span(expr),
            });
            return;
        }
        self.mismatch(expected, found, self.span(expr), expected_from);
    }

    /// Makes `expr`, of type `found`, usable where `expected` is required, returning whether
    /// it is. A `T` is also usable as a `T?`: it's wrapped.
    fn coerce(&mut self, expr: Id<Expr>, expected: TypeId, found: TypeId) -> bool {
        if self.u.unify(expected, found).is_ok() {
            return true;
        }
        let expected = self.u.shallow(expected);
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
        let mut ty = self.infer_inner(id, in_place);
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
            Expr::Int(_) => self.u.fresh(VarKind::Int),
            Expr::Float(_) => self.u.fresh(VarKind::Float),
            Expr::Bool(_) => bool,
            // A `str`, or a `cstr` where C needs one.
            Expr::Str(_) => self.u.fresh(VarKind::Text),
            Expr::None => {
                let inner = self.u.fresh(VarKind::Any);
                self.u.intern(Ty::Optional(inner))
            }
            // A collection: an array, or a slice where one is expected.
            Expr::Array(items) => {
                let element = self.u.fresh(VarKind::Any);
                for &item in items {
                    self.expect(item, element, None);
                }
                self.u.fresh(VarKind::Collection {
                    element,
                    len: items.len() as u64,
                })
            }
            Expr::Cast { value, ty } => {
                let target = self.lower(*ty);
                let found = self.infer(*value);
                // The value converts at run time: `300 as u8` is the `i32` 300, wrapped to 44.
                let number = |c: &Self, ty: TypeId| {
                    matches!(
                        c.u.get(c.u.shallow(ty)),
                        Ty::Int(_) | Ty::Float(_) | Ty::Error
                    ) || matches!(c.u.var_kind(ty), Some(VarKind::Int | VarKind::Float))
                };
                if !number(self, found) || !number(self, target) {
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
                self.u.fresh(VarKind::Collection {
                    element,
                    len: *count,
                })
            }
            Expr::Fallback { optional, fallback } => {
                let ty = self.infer_optional(*optional);
                let inner = self.optional_inner(ty, *optional);
                let from = self.span(*optional);
                self.expect(*fallback, inner, Some(from));
                inner
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
                Some((def, DefKind::Param { .. } | DefKind::Local(_) | DefKind::Out(_))) => {
                    let ty = self.locals.get(&def).copied().unwrap_or(error);
                    // Passed on, a `&mut` can change what's behind it.
                    if !in_place && self.is_mut_ref(ty) {
                        self.forget(|key| key.root == def && !key.path.is_empty());
                    }
                    ty
                }
                Some((_, DefKind::Func(func))) => {
                    let (sig, args) = self.instantiate(*func);
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
                self.binary(id, *op, lhs_ty, rhs_ty)
            }

            // `!x` of an optional is `x == none`.
            Expr::Not(_) => {
                self.check_condition(id);
                bool
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
                if !self.is_place(*target) {
                    self.errors.push(TypeError::InvalidAssignTarget {
                        span: self.span(*target),
                    });
                }
                // The target is the optional itself, not the value it held: `x = none` is fine.
                let assigned = self.key(*target);
                let (lifted, kept) = std::mem::take(&mut self.narrowed)
                    .into_iter()
                    .partition(|n| assigned.as_ref().is_some_and(|a| n.key.same(a)));
                self.narrowed = kept;
                let target_ty = self.infer(*target);
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

            Expr::Call { callee, args } => {
                match self.names.expr(id).map(|def| &self.names.def(def).kind) {
                    Some(DefKind::Func(func)) => {
                        let func = *func;
                        let (sig, type_args) = self.instantiate(func);
                        if !type_args.is_empty() {
                            self.type_args.insert(id, type_args);
                        }
                        self.check_args(id, &callee.to_string(), func, &sig.params, args);
                        sig.ret
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
                let receiver_ty = self.infer(*receiver);
                if let Some(ty) = self.array_method(id, *receiver, receiver_ty, method, args) {
                    return ty;
                }
                let Some(owner) = self.struct_of(receiver_ty, *receiver) else {
                    args.iter().for_each(|&a| {
                        self.infer(a);
                    });
                    return error;
                };
                let Some(def) = self.names.method(owner, &method.name) else {
                    let ty = self.describe(receiver_ty);
                    self.errors.push(TypeError::UnknownMethod {
                        ty,
                        method: method.name.to_string(),
                        span: method.span.clone(),
                    });
                    args.iter().for_each(|&a| {
                        self.infer(a);
                    });
                    return error;
                };
                let DefKind::Func(func) = self.names.def(def).kind else {
                    unreachable!("methods are functions")
                };
                self.methods.insert(id, def);

                let (sig, type_args) = self.instantiate(func);
                if !type_args.is_empty() {
                    self.type_args.insert(id, type_args);
                }
                // The receiver is the first (`self`) parameter.
                match sig.params.split_first() {
                    Some((&self_ty, rest)) => {
                        if self.u.unify(self_ty, receiver_ty).is_err() {
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
                // A range's two ends.
                let mut viewed = self.u.shallow(object_ty);
                if let Ty::Ref { to, .. } = *self.u.get(viewed) {
                    viewed = self.u.shallow(to);
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
                                span: field.span.clone(),
                            });
                            return error;
                        }
                    };
                    self.fields.insert(id, index);
                    return int;
                }
                let Some(owner) = self.struct_of(object_ty, *object) else {
                    return error;
                };
                let DefKind::Struct(s) = self.names.def(owner).kind else {
                    unreachable!("struct_of returns structs")
                };
                let fields = &self.ast.structs[s].fields;
                match fields.iter().position(|f| f.name.name == field.name) {
                    Some(index) => {
                        self.fields.insert(id, index);
                        self.structs[s.id][index]
                    }
                    None => {
                        self.errors.push(TypeError::UnknownField {
                            ty: self.names.def(owner).name.to_string(),
                            field: field.name.to_string(),
                            available: fields.iter().map(|f| f.name.name.to_string()).collect(),
                            span: field.span.clone(),
                        });
                        error
                    }
                }
            }

            Expr::Construct { fields: given, .. } => {
                let Some(def) = self.names.expr(id) else {
                    given.iter().for_each(|(_, v)| {
                        self.infer(*v);
                    });
                    return error;
                };
                let DefKind::Struct(s) = self.names.def(def).kind else {
                    unreachable!("resolver only binds constructors to structs")
                };
                let struct_name = self.names.def(def).name.to_string();
                // Only C can make one: som doesn't know what's inside.
                if self.ast.structs[s].handle {
                    self.errors.push(TypeError::ConstructHandle {
                        name: struct_name,
                        span: self.span(id),
                    });
                    given.iter().for_each(|(_, v)| {
                        self.infer(*v);
                    });
                    return self.u.intern(Ty::Struct(def));
                }
                let declared = &self.ast.structs[s].fields;

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
                            let expected = self.structs[s.id][index];
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
                                span: name.span.clone(),
                            });
                            self.infer(*value);
                        }
                    }
                }
                for (field, _) in declared.iter().zip(&seen).filter(|(_, seen)| !**seen) {
                    self.errors.push(TypeError::MissingField {
                        ty: struct_name.clone(),
                        field: field.name.name.to_string(),
                        span: self.span(id),
                    });
                }
                self.u.intern(Ty::Struct(def))
            }

            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                self.expect(*condition, bool, None);
                let then_ty = self.infer(*then);
                let otherwise_ty = self.infer(*otherwise);
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

            Expr::Block { stmts, value } => {
                // Like `check_stmts`, but what's narrowed in here lasts into the final value.
                let outer: HashSet<usize> = self.narrowed.iter().map(|n| n.id).collect();
                for &stmt in stmts {
                    self.check_stmt(stmt);
                }
                let ty = match value {
                    Some(value) => self.infer(*value),
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
    ) -> Option<TypeId> {
        let error = self.u.table.error;
        // `[1, 2, 3].len()`: a collection nothing else decides is an array, and a string a `str`.
        match self.u.var_kind(receiver_ty) {
            Some(VarKind::Collection { element, len }) => {
                let array = self.u.intern(Ty::Array { element, len });
                let _ = self.u.unify(receiver_ty, array);
            }
            Some(VarKind::Text) => {
                let str = self.u.table.str_ref;
                let _ = self.u.unify(receiver_ty, str);
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
            self.errors.push(TypeError::UnknownMethod {
                ty: self.describe(array),
                method: method.name.to_string(),
                span: method.span.clone(),
            });
            infer_args(self);
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

    /// Whether values of `ty` are copied rather than moved. A type that isn't known yet is
    /// taken to be.
    fn is_copy(&self, ty: TypeId) -> bool {
        match *self.u.get(self.u.shallow(ty)) {
            Ty::Struct(def) => match self.names.def(def).kind {
                DefKind::Struct(s) => self.ast.structs[s].copy,
                _ => true,
            },
            Ty::Ref { mutable: true, .. } => false,
            Ty::Optional(inner) | Ty::Array { element: inner, .. } => self.is_copy(inner),
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
            } else if self.u.shallow(ty) == self.u.table.str_ref {
                self.errors.push(TypeError::StrToC {
                    span: self.span(extra),
                });
            }
        }
    }

    /// The struct `ty` is, looking through one reference. Reports an error if it isn't one.
    fn struct_of(&mut self, ty: TypeId, expr: Id<Expr>) -> Option<DefId> {
        let mut ty = self.u.shallow(ty);
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
        }
        match self.u.get(ty) {
            Ty::Struct(def) => Some(*def),
            Ty::Error => None,
            Ty::Var(_) if self.u.var_kind(ty) == Some(VarKind::Any) => {
                self.errors.push(TypeError::CannotInfer {
                    span: self.span(expr),
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
            Some(Immutable::BehindSharedReference) => TypeError::BehindSharedReference {
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
                    DefKind::Local(stmt) => matches!(
                        self.ast.stmts[stmt],
                        Stmt::Let { mutable: true, .. } | Stmt::For { mutable: true, .. }
                    ),
                    DefKind::Param { func, index } => {
                        let param = &self.ast.funcs[func].params[index];
                        param.mutable || param.out
                    }
                    _ => true,
                };
                (!mutable).then_some(Immutable::Variable(def))
            }
            // Through a reference, what counts is the reference: `&mut` allows it, even in a
            // variable that isn't `mut`.
            Expr::Field { object, .. } => match self.reference(*object) {
                Some(true) => None,
                Some(false) => Some(Immutable::BehindSharedReference),
                None => self.immutable(*object),
            },
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
                    DefKind::Local(_) | DefKind::Out(_) | DefKind::Param { .. }
                )
            }),
            Expr::Field { object, .. } => self.is_place(*object),
            Expr::Deref(_) => true,
            _ => false,
        }
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

        let result = if is_arithmetic(op) { lhs } else { bool };
        if self.u.unify(lhs, rhs).is_err() {
            self.invalid_operands(id, op, lhs, rhs);
            return if is_arithmetic(op) { error } else { bool };
        }
        match self.operand_ok(op, lhs) {
            Some(true) => {}
            Some(false) => self.invalid_operands(id, op, lhs, rhs),
            None => self.deferred_ops.push((id, op, lhs, rhs)),
        }
        result
    }

    /// Whether `op` is defined on operands of type `ty`, or `None` if `ty` isn't known yet.
    fn operand_ok(&self, op: BinOp, ty: TypeId) -> Option<bool> {
        if let Some(kind) = self.u.var_kind(ty) {
            return match kind {
                VarKind::Any => None,
                VarKind::Int | VarKind::Float => Some(true),
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
            BinOp::Equals | BinOp::NotEquals => matches!(
                ty,
                Ty::Int(_) | Ty::Float(_) | Ty::Char | Ty::Bool | Ty::Pointer
            ),
            BinOp::And | BinOp::Or => matches!(ty, Ty::Bool),
        })
    }
}

fn is_arithmetic(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide | BinOp::Modulo
    )
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
    BehindSharedReference,
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
                    DefKind::Local(_) | DefKind::Param { .. } | DefKind::Out(_)
                )
                .then(|| Key {
                    root: def,
                    path: Vec::new(),
                })
            }
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
                Stmt::Break(_) | Stmt::Continue(_) => {}
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
            Expr::Hole
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
            Expr::Not(e) | Expr::ArrayRepeat { value: e, .. } | Expr::Cast { value: e, .. } => {
                self.forget_changed_by(*e, false)
            }
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
        }
    }
}
