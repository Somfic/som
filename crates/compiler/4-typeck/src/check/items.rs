use std::collections::HashSet;

use som_ast::{Expr, Func, Id};
use som_lang::LangTrait;
use som_resolve::DefKind;

use super::Checker;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;
use crate::{Signature, Typed};

impl Checker<'_> {
    pub(super) fn check_program(&mut self) {
        self.check_items(None);
    }

    /// Checks the program; or, for an instance, everything about it but its functions' and
    /// constants' bodies, then only function `only`'s body.
    pub(super) fn check_items(&mut self, only: Option<Id<Func>>) {
        self.lower_value_params();
        self.check_aliases();
        // Struct fields and function signatures first, so bodies can use them in any order.
        self.lower_structs();
        // Impl blocks' targets and associated types, which signatures can name.
        self.build_impls();
        // After the impls, so `I::Item` in a bound's arguments is known for any `I`.
        self.lower_bound_args();
        self.lower_signatures();
        self.check_impls();
        self.check_requires();
        self.check_bodies(only);
        self.settle_trait_calls();
        // After the bodies, which decide unwritten return types and what results are used as.
        self.resolve_pending_calls();
        self.check_overloads();
        self.check_called();
        self.check_dyns();
        self.check_obligations();
        self.check_deferred();
    }

    /// The types of value parameters, which the types of fields and parameters can use.
    fn lower_value_params(&mut self) {
        let ast = self.ast;
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
                super::traits::impl_blocks(ast)
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
    }

    fn check_aliases(&mut self) {
        // Every type alias, used or not, so one that refers to itself is reported.
        // Its parameters stand for themselves.
        for def in self.names.aliases().to_vec() {
            if let DefKind::Alias { ty } = self.names.def(def).kind {
                self.expanding.push(def);
                self.lower(ty);
                self.expanding.pop();
            }
        }
    }

    fn lower_structs(&mut self) {
        let ast = self.ast;
        for (_, s) in ast.structs.iter() {
            // An enum's payload: every variant's fields, one variant after another.
            let fields = (s.fields.iter().chain(s.payload()))
                .map(|f| self.lower(f.ty))
                .collect();
            self.records.structs.push(fields);
        }
        for (id, s) in ast.structs.iter() {
            let common = self.common_fields(id, s);
            self.records.common.push(common);
        }
    }

    fn lower_signatures(&mut self) {
        let ast = self.ast;
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
                    self.hints.omitted_args_ok = true;
                    let ret = self.lower(ty);
                    self.hints.omitted_args_ok = false;
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
    }

    fn check_requires(&mut self) {
        let ast = self.ast;
        // `@where i < self.len`: conditions, in terms of the parameters.
        let bool = self.u.table.bool;
        for (id, func) in ast.funcs.iter() {
            self.current_func = Some(id);
            for clause in &func.requires {
                self.expect(clause.condition, bool, None);
            }
        }
        self.current_func = None;
    }

    fn check_bodies(&mut self, only: Option<Id<Func>>) {
        let ast = self.ast;
        // Each with a name: one written in place is checked there, with what's expected of it.
        // For an instance, as they're used.
        for (id, c) in ast.consts.iter() {
            if !c.in_place && only.is_none() {
                self.check_const(id);
            }
        }
        for (id, func) in ast.funcs.iter() {
            if func.body.is_some() && only.is_none_or(|only| only == id) {
                self.check_func(id);
                self.settle(func.name.span.clone());
            }
        }
        // Once bodies are checked: what a function gives can be what its body is.
        if only.is_none() {
            self.check_listeners();
            self.check_tests();
        }
        self.current_func = None;
    }

    fn check_deferred(&mut self) {
        // Conversions chosen once what they convert is known. If it still isn't, that's
        // reported where it's made, not here.
        for (expr, expected, found, from) in std::mem::take(&mut self.deferred.conversions) {
            if self.from_conversion(expr, expected, found)
                || self.conversion_undecided(expected, found)
            {
                continue;
            }
            self.mismatch_at(expr, expected, found, from);
        }

        // `{ draw(); end() }`: a call giving nothing ends with a `;`, as a statement; only a
        // value is left without one.
        for (call, ty) in std::mem::take(&mut self.deferred.tail_calls) {
            if matches!(self.u.shallow_ty(ty), Ty::Unit) {
                self.errors.push(TypeError::UnitTail {
                    span: self.span(call),
                });
            }
        }
        // `"total: {n}"`: what's written in has to be shown as text. Through references.
        if let Some(display) = self.names.lang(LangTrait::Display) {
            for (expr, ty) in std::mem::take(&mut self.deferred.displayed) {
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
        for (expr, op, lhs, rhs) in std::mem::take(&mut self.deferred.ops) {
            if self.trait_op(expr, op, lhs).is_some() {
                continue;
            }
            if self.operand_ok(op, lhs) == Some(false) || self.u.var_kind(lhs) == Some(VarKind::Any)
            {
                self.invalid_operands(expr, op, lhs, rhs);
            }
        }
    }

    /// Checks function `id`'s body, once: in order, or sooner, when a call needs the return
    /// type it decides because it isn't written. Whatever the body being checked had going
    /// is put back afterwards.
    pub(super) fn check_func(&mut self, id: Id<Func>) {
        let func = &self.ast.funcs[id];
        let Some(body) = func.body else { return };
        if !self.checked.insert(id) {
            return;
        }
        let returns = self.returns.take();
        let narrowed = std::mem::take(&mut self.narrowed);
        let outer = (
            self.current_func.replace(id),
            std::mem::take(&mut self.loop_depth),
            self.take_hints(),
        );
        // A member only some instances have (`@if N >= 3 { prop z … }`), checked generically:
        // for its type, which its body can decide. What's wrong in it is said per instance.
        let quiet = !func.when.is_empty()
            && !(self.names.type_params(id).iter()).any(|p| self.bound.contains_key(p));
        // Its conditions, each once: a `bool` worked out while compiling, `N >= 3` or `T == f32`.
        if quiet {
            for &condition in &func.when {
                if self.exprs[condition.id].is_none() {
                    let bool = self.u.table.bool;
                    self.expect(condition, bool, None);
                }
            }
        }
        let before = self.errors.len();
        self.check_body(id, func, body);
        self.check_moves_out(body);
        self.check_main_params(id, func);
        self.check_main_returns(id, func, body);
        // After the body, which decides a return type that isn't written.
        if func.prop {
            self.check_prop(id, func);
        }
        if quiet {
            self.errors.truncate(before);
            self.records.per_instance.insert(id);
        }
        self.returns = returns;
        self.narrowed = narrowed;
        (self.current_func, self.loop_depth, self.hints) = outer;
    }

    /// Each `@on fail` function is called with a `&Failure`, and gives nothing back.
    fn check_listeners(&mut self) {
        let failure = self.names.lang_struct(som_lang::LangStruct::Failure);
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

    /// Each `@test` takes nothing and gives nothing back: the runner calls it with nothing,
    /// and a failure is what fails it.
    fn check_tests(&mut self) {
        for &id in self.names.tests() {
            let sig = &self.sigs[id.id];
            let ret = self.u.shallow(sig.ret);
            // Still unknown: its body never gets to the end, like `@unreachable;`.
            let gives = matches!(self.u.table.get(ret), Ty::Unit | Ty::Var(_));
            let func = &self.ast.funcs[id];
            if !sig.params.is_empty() || !gives || !func.generics.is_empty() {
                self.errors.push(TypeError::TestSignature {
                    span: func.name.span.clone(),
                });
            }
        }
    }

    /// Checks constant `id`'s value, once: in order, or sooner, when a value naming it needs
    /// its type. Without a written type, it's what its value would be with nothing else
    /// said: `60` is an `i32`, wherever it's used.
    pub(super) fn check_const(&mut self, id: Id<som_ast::Const>) -> TypeId {
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
            self.hints.expected.take(),
            self.hints.expected_here.take(),
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
            self.hints.expected,
            self.hints.expected_here,
        ) = outer;
        ty
    }

    /// What `main` gives is the program's exit code: an integer, or nothing. A script ending
    /// in `"done"` gives a `string`, which can't be one.
    fn check_main_returns(&mut self, id: Id<Func>, func: &Func, body: Id<Expr>) {
        let def = self.names.func(id);
        let top_level = self.ast.modules[self.names.def(def).module].name.is_empty()
            && self.names.impl_of(id).is_none();
        if &*func.name.name != som_lang::ENTRY_POINT || !top_level {
            return;
        }
        // A literal it ends in: text is a `string`, a number what it would be anyway.
        let ret = self.sigs[id.id].ret;
        self.text_as_string(ret);
        if self.u.var_kind(ret) == Some(VarKind::Float) {
            self.default_numbers(ret);
        }
        let ret = self.u.shallow(ret);
        // `fn main() -> !`: failing, it says why and exits with 1.
        if let Some(ok) = self.outcome_inner(ret)
            && *self.u.shallow_ty(ok) == Ty::Unit
        {
            return;
        }
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
        if &*func.name.name != som_lang::ENTRY_POINT || !top_level {
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
        self.hints.expected = Some(ret);
        let body_ty = self.infer(body);
        // A body without a final value is fine if no path gets to its end: every one returns.
        if let Expr::Block { stmts, value: None } = &self.ast.exprs[body] {
            if self.leaves(stmts) {
                return;
            }
            // `@if N >= 3 { return a; } else @if N < 3 { return b; }`: whether each instance
            // returns is said checking it, where they're worked out.
            let instance = (self.names.type_params(id).iter()).any(|p| self.bound.contains_key(p));
            if self.records.per_instance.contains(&id) && !instance {
                return;
            }
            // Some path gets to the end with nothing to return: fine for `()`, and `!`.
            let unit = self.u.table.unit;
            let fails_or_unit = self.outcome_inner(ret).is_some_and(|ok| self.u.unify(ok, unit).is_ok());
            if !fails_or_unit && self.u.unify(ret, unit).is_err() {
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
        if matches!(self.u.shallow_ty(ret), Ty::Unit) {
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
            let (min, max) = (int.min(), int.max());
            let fits = match *n < 0 {
                true => *n >= min,
                false => *n as u128 <= max,
            };
            if !fits {
                self.errors.push(TypeError::LiteralOutOfRange {
                    value: *n,
                    ty: int.name().to_string(),
                    max,
                    min,
                    span: self.span(id),
                    type_arg: false,
                });
            }
        }
    }

    /// Replaces all inference variables, producing the final tables.
    pub(super) fn finish(mut self) -> (Typed, Vec<TypeError>) {
        let mut reported = HashSet::new();
        let error = self.u.table.error;
        // `@unreachable` where nothing decided a type: as a statement, it's `()`.
        for ty in std::mem::take(&mut self.deferred.unreachables) {
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
        let made_dyn = std::mem::take(&mut self.made_dyn)
            .into_iter()
            .map(|(expr, ty)| (expr, self.u.finish(ty, &mut ignore)))
            .collect();
        let closures = std::mem::take(&mut self.records.closures);
        self.records.closures = (closures.into_iter())
            .map(|(expr, mut sig)| {
                for ty in sig.params.iter_mut().chain([&mut sig.ret]) {
                    *ty = self.u.finish(*ty, &mut ignore);
                }
                for capture in &mut sig.captures {
                    capture.ty = self.u.finish(capture.ty, &mut ignore);
                }
                (expr, sig)
            })
            .collect();
        self.check_literals_fit(&exprs);
        let typed = Typed {
            table: self.u.table,
            exprs,
            locals,
            funcs,
            conversions,
            type_args,
            sliced,
            made_dyn,
            impls: self.impls,
            const_values: self.known,
            records: self.records,
            failures: Default::default(),
        };
        (typed, self.errors)
    }
}
