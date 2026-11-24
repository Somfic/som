use std::collections::HashSet;

use som_ast::{BinOp, Expr, Flow, Id, Stmt, TypeExpr};
use som_resolve::DefKind;

use super::Checker;
use super::narrow::Key;
use crate::error::{TypeError, Unused};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;
use crate::{ArrayMethod, Loop, PointerMethod, Take};

impl Checker<'_> {
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
        self.loop_depth += 1;
        self.check_stmts(body);
        self.loop_depth -= 1;
    }

    pub(super) fn check_stmt(&mut self, id: Id<Stmt>) {
        match &self.ast.stmts[id] {
            Stmt::Use(_) => {}
            // Checked on its own, apart from the function around it.
            Stmt::Const(c) => {
                self.check_const(*c);
            }
            Stmt::Let { ty, value, .. } => self.check_let(id, *ty, *value),
            // `@for f in T` over a generic type: checked for each instance, with `T`'s struct's copy.
            Stmt::Expr(_) if let Some(per_type) = self.ast.per_type.get(&id) => {
                self.check_per_type(id, per_type)
            }
            Stmt::Expr(expr) => self.check_expr_stmt(id, *expr),
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
                // Made of a constant worked out by running code: checked again once it is.
                if self.needs_unknown(*iterable) {
                    return;
                }
                self.errors.push(TypeError::NotSettled {
                    what: "what an `@for` loops over",
                    hint: "loop over a range of literals, constants and a generic function's \
                           value parameters, or over an array written out",
                    span: self.span(*iterable),
                });
                self.infer(*iterable);
            }
            Stmt::For { iterable, body, .. } => self.check_for(id, *iterable, body),
            Stmt::Return(value) => self.check_return(id, *value),
            Stmt::Break(_) | Stmt::Continue(_) if self.loop_depth == 0 => {
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
            // `@if GPU { … }`: both branches are compiled, so both are checked.
            Stmt::If {
                condition,
                then,
                otherwise,
            } if self.ast.gpu_ifs.contains_key(&id) => {
                self.check_if(*condition, then, otherwise.as_deref())
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } if self.ast.settled.contains(&id) => {
                self.check_settled_if(id, *condition, then, otherwise.as_deref())
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => self.check_if(*condition, then, otherwise.as_deref()),
        }
    }

    fn check_if(&mut self, condition: Id<Expr>, then: &[Id<Stmt>], otherwise: Option<&[Id<Stmt>]>) {
        let facts = self.check_condition(condition);
        let (_, then_kept) = self.narrowed_during(facts.when_true, |c| c.check_stmts(then));
        let otherwise_kept = match otherwise {
            Some(otherwise) => {
                self.narrowed_during(facts.when_false, |c| c.check_stmts(otherwise))
                    .1
            }
            None => facts.when_false,
        };
        // If one branch leaves, the rest of the block only runs after the other:
        // `if v == none { return 0; }` narrows `v` until the end of the block.
        let then_leaves = self.leaves(then);
        let otherwise_leaves = otherwise.as_ref().is_some_and(|o| self.leaves(o));
        if then_leaves && !otherwise_leaves {
            self.narrowed.extend(otherwise_kept);
        } else if otherwise_leaves && !then_leaves {
            self.narrowed.extend(then_kept);
        }
    }

    fn check_for(&mut self, id: Id<Stmt>, iterable: Id<Expr>, body: &[Id<Stmt>]) {
        // A later iteration sees what the body changes.
        self.forget_changed_in(body);
        let ty = self.infer(iterable);
        let element = self.loop_element(id, iterable, ty);
        if let Some(def) = self.names.local(id) {
            self.locals.insert(def, element);
        }
        self.check_loop(body);
    }

    fn check_let(&mut self, id: Id<Stmt>, ty: Option<Id<TypeExpr>>, value: Id<Expr>) {
        let ty = match ty {
            Some(annotation) => {
                self.hints.omitted_args_ok = true;
                let expected = self.lower(annotation);
                self.hints.omitted_args_ok = false;
                let from = self.ast.types.span(annotation).clone();
                self.expect(value, expected, Some(from));
                expected
            }
            // `let a = "bob"`: a `string`, whatever it's used as later. Only text
            // passed straight to C becomes a `cstr`.
            None => {
                let ty = self.infer(value);
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
            self.narrow_if_assigned_a_value(Some(key), ty, value);
        }
    }

    fn check_per_type(&mut self, id: Id<Stmt>, per_type: &som_ast::PerType) {
        let name = &per_type.of.name().name;
        let param = (self.current_func).and_then(|f| {
            (self.names.type_params(f).iter().copied())
                .find(|&p| &*self.names.def(p).name == &**name)
        });
        let Some(param) = param else {
            self.errors.push(TypeError::ReflectNotStruct {
                name: name.to_string(),
                span: per_type.of.span(),
            });
            return;
        };
        let Some(&ty) = self.bound.get(&param) else {
            self.records.regions.insert(id);
            if let Some(func) = self.current_func {
                self.records.per_instance.insert(func);
            }
            return;
        };
        let strukt = match self.u.get(ty) {
            Ty::Struct(def, _) => match self.names.def(*def).kind {
                DefKind::Struct(s) => Some(s),
                _ => None,
            },
            _ => None,
        };
        match strukt.and_then(|s| per_type.arms.iter().find(|(a, _)| *a == s)) {
            Some(&(_, arm)) => {
                self.records.chosen_arms.insert(id, arm);
                self.infer(arm);
            }
            None => self.errors.push(TypeError::ReflectNotStruct {
                name: self.describe(ty).plain().trim_matches('`').to_string(),
                span: per_type.of.span(),
            }),
        }
    }

    fn check_expr_stmt(&mut self, id: Id<Stmt>, expr: Id<Expr>) {
        // A `match` whose value isn't used can have arms of different types.
        self.hints.discarding = matches!(self.ast.exprs[expr], Expr::Match { .. });
        let before = self.errors.len();
        let ty = self.infer(expr);
        // `{ begin(); end() }` of a loop or `if`: the last call, without its `;`.
        if self.ast.unterminated.contains(&id)
            && matches!(
                self.ast.exprs[expr],
                Expr::Call { .. } | Expr::MethodCall { .. }
            )
        {
            self.deferred.tail_calls.push((expr, ty));
        }
        self.hints.discarding = false;
        // Once what's wrong with it is fixed, it may not be what's meant either.
        if self.errors.len() == before {
            self.check_unused(expr);
        }
    }

    fn check_return(&mut self, id: Id<Stmt>, value: Option<Id<Expr>>) {
        let (ret, from) = self
            .returns
            .clone()
            .expect("statements are inside a function");
        match value {
            Some(value) => self.expect(value, ret, from),
            None => {
                let unit = self.u.table.unit;
                // `return;` of a `-> !`: done, without failing.
                if self.outcome_inner(ret).is_some_and(|ok| self.u.unify(ok, unit).is_ok()) {
                    return;
                }
                if self.u.unify(ret, unit).is_err() {
                    self.errors.push(TypeError::ReturnWithoutValue {
                        ty: self.describe(ret),
                        span: self.ast.stmts.span(id).clone(),
                    });
                }
            }
        }
    }

    fn check_settled_if(
        &mut self,
        id: Id<Stmt>,
        condition: Id<Expr>,
        then: &[Id<Stmt>],
        otherwise: Option<&[Id<Stmt>]>,
    ) {
        let bool = self.u.table.bool;
        self.expect(condition, bool, None);
        // `@if target::OS == …`, decided while loading.
        let decided = self.ast.decided.get(&id).copied();
        match decided.or_else(|| self.settled_bool(condition)) {
            Some(holds) => {
                self.records.settled_ifs.insert(id, holds);
                match (holds, otherwise) {
                    (true, _) => self.check_stmts(then),
                    (false, Some(otherwise)) => self.check_stmts(otherwise),
                    (false, None) => {}
                }
            }
            // Made of a constant worked out by running code: checked again once it is.
            None if self.needs_unknown(condition) => {}
            // `@if N >= 3`: checked for each instance of the function, where `N` is
            // known. What's in it can't change what's around it here.
            None if self.over_params(condition) => {
                self.records.regions.insert(id);
                if let Some(func) = self.current_func {
                    self.records.per_instance.insert(func);
                }
                self.forget_changed_in(then);
                if let Some(otherwise) = otherwise {
                    self.forget_changed_in(otherwise);
                }
            }
            None => self.errors.push(TypeError::NotSettled {
                what: "an `@if`'s condition",
                hint: "use literals and constants; a generic function's parameters \
                           can't be used in one yet",
                span: self.span(condition),
            }),
        }
    }

    /// Reports statement `expr` if its value is thrown away and computing it does nothing
    /// else: `x == 5;`, `a + b;`. A function's result isn't: calling it may be the point.
    fn check_unused(&mut self, expr: Id<Expr>) {
        let ty = self.exprs[expr.id].map(|t| self.u.shallow(t));
        if ty.is_none_or(|t| matches!(self.u.get(t), Ty::Unit | Ty::Error)) {
            return;
        }
        let span = self.span(expr);
        // `save(path);` of a `save` that can fail: the failure would go unseen.
        if let Some(t) = ty
            && self.outcome_inner(t).is_some()
        {
            self.errors.push(TypeError::UnusedValue {
                kind: Unused::Failure,
                span,
            });
            return;
        }
        let kind = match &self.ast.exprs[expr] {
            Expr::Binary {
                op: BinOp::Equals, ..
            } => {
                let text = span.get_text().replacen("==", "=", 1);
                Unused::Comparison(text)
            }
            Expr::MethodCall { .. } => {
                let pure_pointer = self
                    .records
                    .pointer_methods
                    .get(&expr)
                    .is_some_and(|m| !matches!(m, PointerMethod::Free | PointerMethod::Put));
                let pure_array = self
                    .records
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

    fn flow(&self) -> Flow<'_, Self> {
        Flow {
            ast: self.ast,
            settled: self,
        }
    }

    pub(super) fn leaves(&self, stmts: &[Id<Stmt>]) -> bool {
        self.flow().leaves(stmts)
    }

    pub(super) fn expr_leaves(&self, expr: Id<Expr>) -> bool {
        self.flow().expr_leaves(expr)
    }

    /// What a `for` loop over `iterable` binds each time round; records how it loops.
    fn loop_element(&mut self, stmt: Id<Stmt>, iterable: Id<Expr>, ty: TypeId) -> TypeId {
        let error = self.u.table.error;
        // `for x in [1, 2, 3]`: a collection nothing else decides is an array.
        if let Some(VarKind::Collection { element, len }) = self.u.var_kind(ty) {
            let array = self.u.intern(Ty::Array { element, len });
            let _ = self.u.unify(ty, array);
        }
        let shallow = self.u.shallow(ty);
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
                self.records.loops.insert(stmt, Loop::Range);
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
        self.records.loops.insert(stmt, Loop::Elements { take });
        match take {
            Take::Copy => element,
            Take::Ref | Take::RefMut => self.u.intern(Ty::Ref {
                mutable: take == Take::RefMut,
                to: element,
            }),
        }
    }
}
