//! Anonymous functions: `n => *n > limit`. Each has a type of its own, holding what it
//! captures, and is checked where it's written, reading the variables around it.

use som_ast::{Child, Expr, Id, Param};
use som_resolve::{DefId, DefKind};
use std::collections::HashMap;

use super::Checker;
use crate::{Capture, ClosureSig};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

impl Checker<'_> {
    /// `(a, b) => body`: its parameters' types are written, or what calling it is expected to
    /// take; its body is checked with them, giving what it returns.
    pub(super) fn infer_closure(&mut self, id: Id<Expr>, params: &[Param], body: Id<Expr>) -> TypeId {
        let expected = self.hints.expected_here.take();
        let wanted = expected.and_then(|ty| self.wanted_signature(ty, params.len()));
        let (wanted_params, wanted_ret) = match wanted {
            Some((params, ret)) => (Some(params), Some(ret)),
            None => (None, None),
        };
        let defs = self.names.closure_params(id).to_vec();
        let mut types = Vec::new();
        for (index, (param, &def)) in params.iter().zip(&defs).enumerate() {
            let wanted = wanted_params.as_ref().map(|w| w[index]);
            let ty = match param.ty {
                Some(written) => {
                    let ty = self.lower(written);
                    if let Some(wanted) = wanted {
                        let _ = self.u.unify(wanted, ty);
                    }
                    ty
                }
                None => wanted.unwrap_or_else(|| self.u.fresh(VarKind::Any)),
            };
            self.locals.insert(def, ty);
            types.push(ty);
        }
        let ret = wanted_ret.unwrap_or_else(|| self.u.fresh(VarKind::Any));

        // Its own body: a `return` leaves it, and a `break` can't reach a loop around it.
        let returns = self.returns.replace((ret, None));
        let loop_depth = std::mem::take(&mut self.loop_depth);
        self.open_closures.push(id);
        self.expect(body, ret, None);
        self.open_closures.pop();
        self.loop_depth = loop_depth;
        self.returns = returns;

        let (captures, uses) = self.capture_paths(id, body);
        let owner = self.current_func;
        self.records.closures.insert(
            id,
            ClosureSig {
                owner,
                params: types,
                ret,
                captures,
                uses,
            },
        );
        let def = self.names.closure(id).expect("closures are resolved");
        let args = match owner {
            Some(func) => (self.names.type_params(func).iter())
                .map(|&p| match self.bound.get(&p) {
                    Some(&bound) => bound,
                    None => self.u.intern(Ty::Param(p)),
                })
                .collect(),
            None => Vec::new(),
        };
        self.u.intern(Ty::Closure(def, args))
    }

    /// What closure `closure`, with body `body`, captures, and the expressions in it that read
    /// each. A variable that copies is captured whole; one that doesn't, by the parts of it
    /// read: for each place it's read, `g.camera.pos`, the shortest part that copies,
    /// `g.camera`, or if none does, all of what's read there, lent. A part inside another
    /// that's captured is reached through that one, and a variable read on its own is
    /// captured whole.
    fn capture_paths(&mut self, closure: Id<Expr>, body: Id<Expr>) -> (Vec<Capture>, Vec<(Id<Expr>, usize)>) {
        let mut all = Vec::new();
        self.body_exprs(body, &mut all);
        // Each field read whose object is read for it: the object's place, then its field.
        let mut parent: HashMap<Id<Expr>, Id<Expr>> = HashMap::new();
        for &e in &all {
            if let Expr::Field { object, .. } = &self.ast.exprs[e]
                && self.path_step(e, *object)
            {
                parent.insert(*object, e);
            }
        }
        let field_name = |checker: &Self, e: Id<Expr>| match &checker.ast.exprs[e] {
            Expr::Field { field, .. } => field.name.clone(),
            _ => unreachable!("steps are fields"),
        };

        let mut captures = Vec::new();
        for def in self.names.captures(closure).to_vec() {
            let var_ty = self.locals.get(&def).copied().unwrap_or(self.u.table.error);
            let whole = Capture {
                def,
                path: Vec::new(),
                ty: var_ty,
                expr: None,
            };
            let reads: Vec<Id<Expr>> = (all.iter().copied())
                .filter(|&e| matches!(self.ast.exprs[e], Expr::Var(_)) && self.names.expr(e) == Some(def))
                .collect();
            if self.is_copy(var_ty) || reads.is_empty() {
                captures.push(whole);
                continue;
            }
            let mut parts: Vec<(Vec<Box<str>>, Id<Expr>)> = Vec::new();
            for read in reads {
                let mut at = read;
                let mut path = Vec::new();
                while let Some(&up) = parent.get(&at) {
                    path.push(field_name(self, up));
                    at = up;
                    if self.exprs[up.id].is_some_and(|ty| self.is_copy(ty)) {
                        break;
                    }
                }
                parts.push((path, at));
            }
            if parts.iter().any(|(path, _)| path.is_empty()) {
                captures.push(whole);
                continue;
            }
            // Outermost first: a part inside one already captured is reached through it.
            parts.sort_by_key(|(path, _)| path.len());
            let mut kept: Vec<(Vec<Box<str>>, Id<Expr>)> = Vec::new();
            for (path, expr) in parts {
                if !kept.iter().any(|(k, _)| path.starts_with(k)) {
                    kept.push((path, expr));
                }
            }
            // Then in the order they're written.
            kept.sort_by_key(|(_, expr)| expr.id);
            for (path, expr) in kept {
                let ty = self.exprs[expr.id].unwrap_or(self.u.table.error);
                captures.push(Capture {
                    def,
                    path,
                    ty,
                    expr: Some(expr),
                });
            }
        }

        // Every read of a capture: the variable, or a field path equal to a captured part.
        let mut uses = Vec::new();
        for &e in &all {
            let Some((def, path)) = self.read_path(e, &parent) else {
                continue;
            };
            if let Some(index) = captures.iter().position(|c| c.def == def && c.path == path) {
                uses.push((e, index));
            }
        }
        (captures, uses)
    }

    /// The variable and fields `e` reads, `(g, [camera, pos])` for `g.camera.pos`, if it's a
    /// variable or field reads of one.
    fn read_path(&self, e: Id<Expr>, parent: &HashMap<Id<Expr>, Id<Expr>>) -> Option<(DefId, Vec<Box<str>>)> {
        match &self.ast.exprs[e] {
            Expr::Var(_) => Some((self.names.expr(e)?, Vec::new())),
            Expr::Field { object, field } if parent.get(object) == Some(&e) => {
                let (def, mut path) = self.read_path(*object, parent)?;
                path.push(field.name.clone());
                Some((def, path))
            }
            _ => None,
        }
    }

    /// Whether field read `e` of `object` reads a field of the struct `object` is (through a
    /// reference, as fields are): not a prop, a `len`, an enum's field or what a `#T` shares.
    fn path_step(&self, e: Id<Expr>, object: Id<Expr>) -> bool {
        if !self.records.fields.contains_key(&e)
            || self.records.methods.contains_key(&e)
            || self.records.array_methods.contains_key(&e)
            || self.records.shared.contains(&object)
        {
            return false;
        }
        let Some(mut ty) = self.exprs[object.id] else {
            return false;
        };
        if let Ty::Ref { to, .. } = *self.u.shallow_ty(ty) {
            ty = to;
        }
        match *self.u.shallow_ty(ty) {
            Ty::Struct(def, _) => match self.names.def(def).kind {
                DefKind::Struct(s) => !self.ast.structs[s].is_enum(),
                _ => false,
            },
            _ => false,
        }
    }

    /// Every expression in `expr`, however deep, nested closures' too.
    fn body_exprs(&self, expr: Id<Expr>, out: &mut Vec<Id<Expr>>) {
        out.push(expr);
        self.ast.exprs[expr].for_each_child(|child| match child {
            Child::Expr(e) => self.body_exprs(e, out),
            Child::Stmt(s) => self.stmt_body_exprs(s, out),
        });
    }

    fn stmt_body_exprs(&self, stmt: Id<som_ast::Stmt>, out: &mut Vec<Id<Expr>>) {
        self.ast.stmts[stmt].for_each_child(|child| match child {
            Child::Expr(e) => self.body_exprs(e, out),
            Child::Stmt(s) => self.stmt_body_exprs(s, out),
        });
    }

    /// What a closure written where `expected` is expected should take and give, if that's
    /// known and takes `count` values: a `fn(…) -> …`, or the type parameter of a call it's
    /// given for, `F: fn(&T) -> bool`.
    fn wanted_signature(&mut self, expected: TypeId, count: usize) -> Option<(Vec<TypeId>, TypeId)> {
        let expected = self.u.shallow(expected);
        let signature = match self.u.get(expected) {
            Ty::Fn { .. } => expected,
            Ty::Var(_) => *self.expected_calls.get(&expected)?,
            _ => self.signature_of(expected)?,
        };
        let signature = self.normalize_assoc(signature);
        match self.u.shallow_ty(signature).clone() {
            Ty::Fn { params, ret } if params.len() == count => {
                let params = params.into_iter().map(|p| self.normalize_assoc(p)).collect();
                Some((params, ret))
            }
            _ => None,
        }
    }

    /// What closure `def` takes and gives, as a `fn(…) -> …`, with `args` for the type
    /// parameters of the function it's in.
    pub(super) fn closure_signature(&mut self, def: DefId, args: &[TypeId]) -> Option<TypeId> {
        let DefKind::Closure(expr) = self.names.def(def).kind else {
            return None;
        };
        let sig = (self.records.closures.get(&expr))
            .or_else(|| self.outer_closures.get(&expr))?
            .clone();
        let subst: Vec<(DefId, TypeId)> = match sig.owner {
            Some(func) => (self.names.type_params(func).iter().copied())
                .zip(args.iter().copied())
                .collect(),
            None => Vec::new(),
        };
        let params = (sig.params.iter())
            .map(|&p| self.substitute(p, &subst))
            .collect();
        let ret = self.substitute(sig.ret, &subst);
        Some(self.u.intern(Ty::Fn { params, ret }))
    }
}
