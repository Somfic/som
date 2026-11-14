use crate::error::Unused;
use som_ast::{Ast, BinOp, Expr, Func, Id, Pattern, Primitive, Span, Stmt, TypeExpr, VariantKind};
use som_resolve::{DefId, DefKind, LangTrait, Names};
use std::collections::{HashMap, HashSet};

use crate::error::TypeError;
use crate::ty::{IntTy, Ty, TypeId, Value};
use crate::unify::{Unifier, VarKind};
use crate::{
    ArrayMethod, CommonField, Loop, PatternInfo, PointerMethod, Receiver, Signature, Typed,
};
use som_base::Message;
use som_base::message;

mod matching;
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
    /// The optional places known to hold a value here, innermost last.
    narrowed: Vec<Narrowing>,
    next_narrowing: usize,
    /// Reads of narrowed places: they read the value, not the optional.
    narrowed_reads: HashSet<Id<Expr>>,
    /// Operands read through the reference they are: `self * self` of a `&i32`.
    read_through: HashSet<Id<Expr>>,
    array_methods: HashMap<Id<Expr>, ArrayMethod>,
    pointer_methods: HashMap<Id<Expr>, PointerMethod>,
    sliced: HashMap<Id<Expr>, TypeId>,
    for_loops: HashMap<Id<Stmt>, Loop>,
    /// The function being checked: its return type, and where that's written.
    returns: Option<(TypeId, Option<Span>)>,
    /// The next expression inferred is the object of a field access or dereference.
    in_place: bool,
    /// Operators whose operand type wasn't known yet when they were checked.
    deferred_ops: Vec<(Id<Expr>, BinOp, TypeId, TypeId)>,
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
    /// Calls that more than one function fits, decided once the rest of the program says more
    /// about what their result has to be.
    pending_calls: Vec<PendingCall>,
    /// Functions whose bodies have been checked, or are being.
    checked: HashSet<Id<Func>>,
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
            counts: HashMap::new(),
            value_params: HashMap::new(),
            methods: HashMap::new(),
            fields: HashMap::new(),
            type_args: HashMap::new(),
            wrapped: HashSet::new(),
            narrowed: Vec::new(),
            next_narrowing: 0,
            narrowed_reads: HashSet::new(),
            read_through: HashSet::new(),
            array_methods: HashMap::new(),
            pointer_methods: HashMap::new(),
            sliced: HashMap::new(),
            for_loops: HashMap::new(),
            returns: None,
            in_place: false,
            deferred_ops: Vec::new(),
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
            omitted_args_ok: false,
            impls: Vec::new(),
            current_func: None,
            obligations: Vec::new(),
            trait_ops: HashMap::new(),
            conversions: HashMap::new(),
        }
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
            );
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
        for (id, func) in ast.funcs.iter() {
            if func.body.is_some() {
                self.check_func(id);
                self.settle(func.name.span.clone());
            }
        }
        self.current_func = None;

        // After the bodies, which decide unwritten return types and what results are used as.
        self.current_func = None;
        self.resolve_pending_calls();
        self.check_overloads();
        self.check_obligations();

        for (expr, op, lhs, rhs) in std::mem::take(&mut self.deferred_ops) {
            if self.trait_op(expr, op, lhs) {
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

    fn check_body(&mut self, id: Id<Func>, func: &Func, body: Id<Expr>) {
        let ret = self.sigs[id.id].ret;
        let from = func.returns.map(|t| self.ast.types.span(t).clone());
        self.returns = Some((ret, from.clone()));
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
            self.mismatch(ret, body_ty, self.ast.exprs.span(at).clone(), from);
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
            let min: i128 = match int {
                IntTy::I8 => i8::MIN as i128,
                IntTy::I16 => i16::MIN as i128,
                IntTy::I32 => i32::MIN as i128,
                _ if int.is_signed() => i64::MIN as i128,
                _ => 0,
            };
            let fits = match *n < 0 {
                true => *n as i128 >= min,
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
            narrowed_reads: self.narrowed_reads,
            read_through: self.read_through,
            array_methods: self.array_methods,
            pointer_methods: self.pointer_methods,
            sliced,
            loops: self.for_loops,
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
            // Text is a view of bytes, like a slice: `str` is a reference to them.
            TypeExpr::Primitive(Primitive::Str) => return self.u.table.str_ref,
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
                // `N` in `struct Buffer<N: usize>` stands for a number, not a type.
                Some((def, _)) if self.names.is_value_param(def) => {
                    self.errors.push(TypeError::ValueAsType {
                        what: message::var(&*self.names.def(def).name).into(),
                        span: self.ast.types.span(id).clone(),
                    });
                    Ty::Error
                }
                Some((def, DefKind::TypeParam { .. } | DefKind::StructParam { .. })) => {
                    Ty::Param(def)
                }
                Some((def, DefKind::ImplParam { .. })) => Ty::Param(def),
                Some((_, DefKind::SelfType { impl_ })) => return self.self_type(impl_),
                Some((_, DefKind::ImplType { impl_, index })) => {
                    return self.impl_type(impl_, index);
                }
                Some((_, DefKind::AssocType { trait_, index })) => {
                    let base = self.names.projection_base(id);
                    return self.assoc_type(base, trait_, index);
                }
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
                len: self.lower_value(*len, None),
            },
            TypeExpr::Int(_) | TypeExpr::Bool(_) | TypeExpr::Binary { .. } => {
                self.errors.push(TypeError::ValueAsType {
                    what: message::code(self.ast.type_text(id)).into(),
                    span: self.ast.types.span(id).clone(),
                });
                Ty::Error
            }
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
        if args.len() != params.len() {
            args.iter().for_each(|&a| {
                self.lower_arg(a, None);
            });
            self.errors.push(TypeError::WrongTypeArgCount {
                name: self.names.def(def).name.to_string(),
                expected: params.len(),
                found: args.len(),
                span,
            });
            return vec![self.u.table.error; params.len()];
        }
        (args.iter().zip(params))
            .map(|(&arg, &param)| self.lower_arg(arg, Some(param)))
            .collect()
    }

    /// An argument for generic parameter `param`: a type, or a value if it's a value parameter.
    fn lower_arg(&mut self, arg: Id<TypeExpr>, param: Option<DefId>) -> TypeId {
        match param.and_then(|p| self.value_params.get(&p).copied()) {
            Some(ty) => self.lower_value(arg, Some(ty)),
            None => self.lower(arg),
        }
    }

    /// A value written where a type argument goes, of type `expected`, or of any integer type
    /// for an array's length: a literal, or a value parameter.
    fn lower_value(&mut self, id: Id<TypeExpr>, expected: Option<TypeId>) -> TypeId {
        let span = self.ast.types.span(id).clone();
        let error = self.u.table.error;
        let (value, found) = match self.ast.types[id] {
            TypeExpr::Int(n) => {
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
                                value: n.min(i64::MAX as u128) as i64,
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
                    _ => (Ty::Value(Value::Int(n)), self.u.fresh(VarKind::Int)),
                }
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

    /// A type for an error message. A type that isn't known yet can't be written
    /// down, so it's described in words instead ("an integer"), still shown as a type.
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
        // A struct's operators are its traits': say which to implement, or for a type
        // parameter, to require.
        let implement = LangTrait::of_operator(op).and_then(|lang| {
            let ty = self.u.get(self.u.shallow(lhs));
            match ty {
                Ty::Param(def) => Some((lang.name(), Some(self.names.def(*def).name.to_string()))),
                Ty::Var(_) | Ty::Error | Ty::Optional(_) => None,
                _ if self.has_builtin(lhs, lang) => None,
                _ => Some((lang.name(), None)),
            }
        });
        let fix = LangTrait::of_operator(op)
            .filter(|_| implement.is_some())
            .and_then(|lang| self.operator_fix(lhs, lang));
        self.errors.push(TypeError::InvalidOperands {
            op: op.symbol().into(),
            lhs: self.describe(lhs),
            rhs: self.describe(rhs),
            span: self.ast.exprs.span(expr).clone(),
            optional,
            implement,
            fix,
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
            Stmt::Use(_) => {}
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
                // A `match` whose value isn't used can have arms of different types.
                self.discarding = matches!(self.ast.exprs[*expr], Expr::Match { .. });
                let before = self.errors.len();
                self.infer(*expr);
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
                let pure_pointer = self.pointer_methods.get(&expr).is_some_and(|m| {
                    !matches!(m, PointerMethod::Free | PointerMethod::Write)
                });
                let pure_array = self.array_methods.get(&expr).is_some_and(|m| {
                    !matches!(m, ArrayMethod::Set)
                });
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
            | Expr::Neg(_) => Unused::Value,
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
            Expr::Block { stmts, value: None } => self.leaves(stmts),
            Expr::Match { arms, .. } => {
                !arms.is_empty() && arms.iter().all(|arm| self.expr_leaves(arm.body))
            }
            Expr::Conditional { then, otherwise, .. } => {
                self.expr_leaves(*then) && self.expr_leaves(*otherwise)
            }
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
            Expr::Conditional { then, otherwise, .. } => {
                self.expr_breaks(*then) || self.expr_breaks(*otherwise)
            }
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
            _ => match self.u.get(viewed).clone() {
                Ty::Array { element, .. } | Ty::Slice(element) => element,
                // Anything that's an `Iterator`, or `Iterable`: what it gives.
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
        // `I::Item` of an `I` that's known by now is what its implementation says.
        let (expected, found) = (self.normalize_assoc(expected), self.normalize_assoc(found));
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
                if !plain_enum && !pointers && (!number(self, found) || !number(self, target)) {
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
                let target_ty = self.infer(*target);
                // After inferring it: whether `p.x` is a field or a property depends on `p`.
                if !self.is_place(*target) {
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

            Expr::Call { callee, args } => {
                let expected = self.expected_here.take();
                if let Some(def) = self.names.expr(id)
                    && self.names.overloads(def).len() > 1
                {
                    return self.overloaded_call(id, callee, def, args, expected);
                }
                match self.names.expr(id).map(|def| &self.names.def(def).kind) {
                    Some(DefKind::Func(func)) => {
                        let func = *func;
                        let (sig, type_args) = self.instantiate(func);
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
                // A trait's method the type has, `x.compare(&y)`, before an array's or a
                // pointer's own.
                if let Some(ty) = self.trait_method_call(id, *receiver, receiver_ty, method, args) {
                    return ty;
                }
                // A built-in type's own method, from `impl i32 { … }`.
                let primitive = self
                    .primitive_owner(receiver_ty)
                    .filter(|&owner| self.names.method(owner, &method.name).is_some());
                if primitive.is_none() {
                    if let Some(ty) = self.array_method(id, *receiver, receiver_ty, method, args)
                    {
                        return ty;
                    }
                    if let Some(ty) = self.pointer_method(id, receiver_ty, method, args) {
                        return ty;
                    }
                }
                // Text written in quotes is a `str` here.
                if primitive.is_some() && self.u.var_kind(receiver_ty) == Some(VarKind::Text) {
                    let _ = self.u.unify(receiver_ty, self.u.table.str_ref);
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
                if primitive.is_none() && lacks {
                    self.errors.push(TypeError::UnknownMethod {
                        ty: self.describe(receiver_ty),
                        method: method.name.to_string(),
                        span: method.span.clone(),
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
                            parens: args.is_empty().then(|| method.span.after().merge(&span.after())),
                        });
                        args.iter().for_each(|&a| {
                            self.infer(a);
                        });
                        return error;
                    }
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
                if let Some(ty) = self.prop_access(id, *object, object_ty, field) {
                    return ty;
                }
                // A range's two ends.
                let mut viewed = self.u.shallow(object_ty);
                if let Ty::Ref { to, .. } = *self.u.get(viewed) {
                    viewed = self.u.shallow(to);
                }
                // `items.len` of something with a method of that name, not a field.
                if matches!(self.u.get(viewed), Ty::Array { .. } | Ty::Slice(_) | Ty::Str)
                    && ArrayMethod::from_name(&field.name).is_some()
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

            Expr::Index { object, index } => self.index(*object, *index),

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

    /// A call `callee(args)` naming function `first` and others with its name: the one whose
    /// parameters fit the arguments and whose result fits `expected`, where that's known; of
    /// several, one that isn't generic. Returns the call's type.
    fn overloaded_call(
        &mut self,
        call: Id<Expr>,
        callee: &som_ast::Path,
        first: DefId,
        args: &[Id<Expr>],
        expected: Option<TypeId>,
    ) -> TypeId {
        // The arguments decide which, so they're worked out first, once.
        let found: Vec<TypeId> = args.iter().map(|&a| self.infer(a)).collect();
        let mut fitting = Vec::new();
        // Ones the arguments fit as they are; only if there are none, ones they fit converted.
        for converting in [false, true] {
            for def in self.names.overloads(first) {
                let DefKind::Func(func) = self.names.def(def).kind else {
                    continue;
                };
                let (sig, type_args) = self.instantiate(func);
                let variadic = self.ast.funcs[func].variadic;
                let count_ok = match variadic {
                    true => found.len() >= sig.params.len(),
                    false => found.len() == sig.params.len(),
                };
                let fits = count_ok
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
        self.finish_call(call, def, func, &sig, type_args, args, &found);
        sig.ret
    }

    /// Whether arguments of types `found` fit parameters `params` as they are, or with
    /// `converting`, also through `From`.
    fn args_fit(&mut self, params: &[TypeId], found: &[TypeId], converting: bool) -> bool {
        (params.iter().zip(found)).all(|(&p, &a)| {
            self.u.could_fit(p, a) || (converting && self.could_convert(p, a))
        })
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
                self.mismatch(param, ty, self.span(arg), from);
            }
        }
        sig.ret
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
                self.mismatch(param, ty, self.span(arg), from);
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
            self.errors.push(TypeError::UnknownMethod {
                ty: self.describe(receiver_ty),
                method: method.name.to_string(),
                span: method.span.clone(),
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
        let mut ty = self.u.shallow(ty);
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
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
            Expr::Field { object, .. } | Expr::Index { object, .. } => match self.reference(*object) {
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
                    DefKind::Local(_)
                        | DefKind::Out(_)
                        | DefKind::Param { .. }
                        | DefKind::Binding(_)
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
                Some((self.u.display(target, self.names), self.impl_target_span(index)))
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
    fn index(&mut self, object: Id<Expr>, index: Id<Expr>) -> TypeId {
        let error = self.u.table.error;
        let object_ty = self.infer_object(object);
        let usize = self.u.table.usize;
        self.expect(index, usize, None);
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
        let Expr::Int(at) = self.ast.exprs[index] else {
            self.errors.push(TypeError::IndexNotConstant {
                span: self.span(index),
                get: self.get_suggestion(object, index),
            });
            return element;
        };
        let len_ty = self.u.normalize(len);
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

    /// `values.get(i)` for `values[i]`, to suggest instead.
    fn get_suggestion(&self, object: Id<Expr>, index: Id<Expr>) -> String {
        format!("{}.get({})", self.span(object).get_text(), self.span(index).get_text())
    }

    /// Whether `receiver_ty`, looking through a reference, could be what the impl block of
    /// method `func` is for, with `type_args` its instantiation. True when that's not known.
    fn impl_target_fits(&mut self, func: Id<Func>, type_args: &[TypeId], receiver_ty: TypeId) -> bool {
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
        // Text written in quotes is a `str` here.
        if self.u.var_kind(object_ty) == Some(VarKind::Text) {
            let _ = self.u.unify(object_ty, self.u.table.str_ref);
        }
        // Of the impl blocks with one, the one for the value's type: `x` of a `Vector<3, T>`.
        let mut chosen = None;
        for &def in &candidates {
            let func = func_of(self, def);
            let (sig, type_args) = self.instantiate(func);
            if self.impl_target_fits(func, &type_args, object_ty) {
                chosen = Some((def, func, sig, type_args));
                break;
            }
        }
        let Some((def, func, sig, type_args)) = chosen else {
            let funcs: Vec<Id<Func>> = candidates.iter().map(|&d| func_of(self, d)).collect();
            self.not_on_impl_target(&funcs, object_ty, field, "property");
            return Some(self.u.table.error);
        };
        self.methods.insert(id, def);
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
        // Values of a type implementing `Eq` compare with its `eq`, `Add` add with its `add`.
        if self.trait_op(id, op, lhs) {
            return result;
        }
        match self.operand_ok(op, lhs) {
            Some(true) => {}
            Some(false) => self.invalid_operands(id, op, lhs, rhs),
            None => self.deferred_ops.push((id, op, lhs, rhs)),
        }
        result
    }

    /// An operand of an arithmetic or comparison operator, of type `ty`: a reference to a
    /// number or `bool` is read through, so `self * self` works for a `&i32`.
    fn through_reference(&mut self, operand: Id<Expr>, ty: TypeId) -> TypeId {
        let Ty::Ref { to, .. } = *self.u.get(self.u.shallow(ty)) else {
            return ty;
        };
        let to = self.u.shallow(to);
        let number = matches!(self.u.get(to), Ty::Int(_) | Ty::Float(_) | Ty::Bool)
            || matches!(self.u.var_kind(to), Some(VarKind::Int | VarKind::Float));
        if !number {
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
            BinOp::Equals | BinOp::NotEquals => {
                matches!(
                    ty,
                    Ty::Int(_) | Ty::Float(_) | Ty::Char | Ty::Bool | Ty::Pointer
                ) || self.is_plain_enum(ty)
            }
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
                Stmt::Break(_) | Stmt::Continue(_) | Stmt::Use(_) => {}
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
            | Expr::ArrayRepeat { value: e, .. }
            | Expr::Cast { value: e, .. } => {
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
            Expr::Match { scrutinee, arms } => {
                self.forget_changed_by(*scrutinee, false);
                for arm in arms {
                    self.forget_changed_by(arm.body, false);
                }
            }
        }
    }
}
