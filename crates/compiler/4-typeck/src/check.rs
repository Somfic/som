use som_ast::{Ast, BinOp, Expr, Func, Id, Span, Stmt, TypeExpr};
use som_resolve::{DefId, DefKind, Names};
use std::collections::{HashMap, HashSet};

use crate::error::TypeError;
use crate::ty::{Ty, TypeId};
use crate::unify::{Unifier, VarKind};
use crate::{Signature, Typed};
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
    /// Operators whose operand type wasn't known yet when they were checked.
    deferred_ops: Vec<(Id<Expr>, BinOp, TypeId, TypeId)>,
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
            deferred_ops: Vec::new(),
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
                .map(|p| match p.ty {
                    Some(ty) => self.lower(ty),
                    None => self.u.fresh(VarKind::Any),
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
        if self.u.unify(ret, body_ty).is_err() {
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

        let typed = Typed {
            table: self.u.table,
            exprs,
            locals,
            funcs,
            structs: self.structs,
            methods: self.methods,
            fields: self.fields,
            type_args,
        };
        (typed, self.errors)
    }

    // --- Types ---

    /// The type a type annotation describes.
    fn lower(&mut self, id: Id<TypeExpr>) -> TypeId {
        let ty = match &self.ast.types[id] {
            TypeExpr::Unit => Ty::Unit,
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
            TypeExpr::Reference { mutable, to, .. } => {
                let to = self.lower(*to);
                Ty::Ref {
                    mutable: *mutable,
                    to,
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
            Ty::Ref { mutable, to } => {
                let to = self.substitute(to, subst);
                self.u.intern(Ty::Ref { mutable, to })
            }
            Ty::Fn { params, ret } => {
                let params = params.iter().map(|&p| self.substitute(p, subst)).collect();
                let ret = self.substitute(ret, subst);
                self.u.intern(Ty::Fn { params, ret })
            }
            _ => ty,
        }
    }

    // --- Errors ---

    /// A type for an error message. A type that isn't known yet can't be written
    /// down, so it's described in words instead ("an integer"), still shown as a type.
    fn describe(&self, ty: TypeId) -> Message {
        let text = match self.u.var_kind(ty) {
            Some(VarKind::Int) => "an integer".to_string(),
            Some(VarKind::Float) => "a float".to_string(),
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
        self.errors.push(TypeError::InvalidOperands {
            op: op.symbol().into(),
            lhs: self.describe(lhs),
            rhs: self.describe(rhs),
            span: self.ast.exprs.span(expr).clone(),
        });
    }

    fn span(&self, expr: Id<Expr>) -> Span {
        self.ast.exprs.span(expr).clone()
    }

    // --- Statements ---

    fn check_stmts(&mut self, stmts: &[Id<Stmt>]) {
        for &stmt in stmts {
            self.check_stmt(stmt);
        }
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
                }
            }
            Stmt::Expr(expr) => {
                self.infer(*expr);
            }
            Stmt::Loop { body } => self.check_stmts(body),
            Stmt::While { condition, body } => {
                let bool = self.u.table.bool;
                self.expect(*condition, bool, None);
                self.check_stmts(body);
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let bool = self.u.table.bool;
                self.expect(*condition, bool, None);
                self.check_stmts(then);
                if let Some(otherwise) = otherwise {
                    self.check_stmts(otherwise);
                }
            }
        }
    }

    // --- Expressions ---

    /// Infers the type of `expr` and requires it to be `expected`.
    fn expect(&mut self, expr: Id<Expr>, expected: TypeId, expected_from: Option<Span>) {
        let found = self.infer(expr);
        if self.u.unify(expected, found).is_err() {
            self.mismatch(expected, found, self.span(expr), expected_from);
        }
    }

    fn infer(&mut self, id: Id<Expr>) -> TypeId {
        let ty = self.infer_inner(id);
        self.exprs[id.id] = Some(ty);
        ty
    }

    fn infer_inner(&mut self, id: Id<Expr>) -> TypeId {
        let table = &self.u.table;
        let (error, unit, bool) = (table.error, table.unit, table.bool);

        match &self.ast.exprs[id] {
            Expr::Hole => error,
            Expr::Int(_) => self.u.fresh(VarKind::Int),
            Expr::Float(_) => self.u.fresh(VarKind::Float),
            Expr::Bool(_) => bool,
            Expr::Str(_) => self.u.table.str_ref,

            Expr::Var(_) => match self
                .names
                .expr(id)
                .map(|def| (def, &self.names.def(def).kind))
            {
                Some((def, DefKind::Param { .. } | DefKind::Local(_))) => {
                    self.locals.get(&def).copied().unwrap_or(error)
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

            Expr::Binary { op, lhs, rhs } => {
                let lhs_ty = self.infer(*lhs);
                let rhs_ty = self.infer(*rhs);
                self.binary(id, *op, lhs_ty, rhs_ty)
            }

            Expr::Not(inner) => {
                self.expect(*inner, bool, None);
                bool
            }

            Expr::Borrow { mutable, expr } => {
                let to = self.infer(*expr);
                self.u.intern(Ty::Ref {
                    mutable: *mutable,
                    to,
                })
            }

            Expr::Deref(inner) => {
                let ty = self.infer(*inner);
                let ty = self.u.shallow(ty);
                match self.u.get(ty).clone() {
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
                let target_ty = self.infer(*target);
                let from = self.span(*target);
                self.expect(*value, target_ty, Some(from));
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
                            found: args.len() + 1,
                            span: self.span(id),
                        });
                    }
                }
                sig.ret
            }

            Expr::Field { object, field } => {
                let object_ty = self.infer(*object);
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
                let declared = &self.ast.structs[s].fields;
                let struct_name = self.names.def(def).name.to_string();

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
                let from = self.span(*then);
                self.expect(*otherwise, then_ty, Some(from));
                then_ty
            }

            Expr::Block { stmts, value } => {
                self.check_stmts(stmts);
                match value {
                    Some(value) => self.infer(*value),
                    None => unit,
                }
            }
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
        if args.len() != params.len() {
            self.errors.push(TypeError::WrongArgCount {
                name: name.to_string(),
                expected: params.len(),
                found: args.len(),
                span: self.span(call),
            });
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return;
        }
        for (i, (&arg, &param)) in args.iter().zip(params).enumerate() {
            let from = self.ast.funcs[func].params[first + i]
                .ty
                .map(|t| self.ast.types.span(t).clone());
            self.expect(arg, param, from);
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

    /// Whether `expr` denotes a memory location that can be assigned to.
    fn is_place(&self, expr: Id<Expr>) -> bool {
        match &self.ast.exprs[expr] {
            Expr::Var(_) => self.names.expr(expr).is_none_or(|def| {
                matches!(
                    self.names.def(def).kind,
                    DefKind::Local(_) | DefKind::Param { .. }
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
