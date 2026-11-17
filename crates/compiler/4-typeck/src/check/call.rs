use std::collections::HashSet;

use som_ast::{Expr, Func, Id, Span, TypeExpr, VariantKind};
use som_base::{Fix, message, msg};
use som_resolve::{DefId, DefKind};

use super::Checker;
use super::matching::holds;
use crate::Signature;
use crate::error::{NotForC, TypeError};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

/// A call more than one function fitted when it was checked: see `resolve_pending_calls`.
pub(super) struct PendingCall {
    call: Id<Expr>,
    name: String,
    args: Vec<Id<Expr>>,
    found: Vec<TypeId>,
    /// The functions still fitting: each one's definition, function, signature with fresh
    /// variables for its type parameters, and those variables.
    fitting: Vec<(DefId, Id<Func>, Signature, Vec<TypeId>)>,
    /// The call's type, until it's decided.
    result: TypeId,
}

impl Checker<'_> {
    pub(super) fn infer_call(
        &mut self,
        id: Id<Expr>,
        callee: &som_ast::Path,
        owner_args: &[Id<TypeExpr>],
        written: &[Id<TypeExpr>],
        args: &[Id<Expr>],
    ) -> TypeId {
        let error = self.u.table.error;
        let expected = self.hints.expected_here.take();
        if let Some(def) = self.names.expr(id)
            && self.names.overloads(def).len() > 1
        {
            return self.overloaded_call(id, callee, def, owner_args, written, args, expected);
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
                    self.infer_all(args);
                } else if args.len() != fields.len() {
                    self.errors.push(TypeError::WrongArgCount {
                        name: callee.to_string(),
                        expected: fields.len(),
                        at_least: false,
                        found: args.len(),
                        span: self.span(id),
                    });
                    self.infer_all(args);
                } else {
                    for ((&arg, &field), declared) in args.iter().zip(&fields).zip(&variant.fields)
                    {
                        let from = self.ast.types.span(declared.ty).clone();
                        self.expect(arg, field, Some(from));
                    }
                }
                ty
            }
            _ => {
                self.infer_all(args);
                error
            }
        }
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
        for prefer_defaults in [false, true] {
            if fitting.len() < 2 {
                break;
            }
            let kept: Vec<_> = (fitting.iter())
                .filter(|(_, _, sig, _)| {
                    (sig.params.iter().zip(&found)).all(|(&param, &arg)| {
                        if prefer_defaults {
                            self.fits_as_default(param, arg)
                        } else {
                            matches!(self.u.shallow_ty(arg), Ty::Optional(_))
                                || !matches!(self.u.shallow_ty(param), Ty::Optional(_))
                        }
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
            self.deferred.calls.push(PendingCall {
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
    pub(super) fn args_fit(
        &mut self,
        params: &[TypeId],
        found: &[TypeId],
        converting: bool,
    ) -> bool {
        (params.iter().zip(found))
            .all(|(&p, &a)| self.u.could_fit(p, a) || (converting && self.could_convert(p, a)))
    }

    pub(super) fn has_unknowns(&self, ty: TypeId) -> bool {
        let ty = self.u.shallow(ty);
        match self.u.get(ty) {
            Ty::Var(_) => true,
            t => t.children().into_iter().any(|c| self.has_unknowns(c)),
        }
    }

    /// For an overloaded call whose result is `result`, not decided yet: keeps the functions
    /// whose return type `keep` accepts, and if that leaves one, takes it now. A `match` on
    /// the call does this with what its arms can match.
    pub(super) fn decide_pending_by(
        &mut self,
        result: TypeId,
        keep: &dyn Fn(&Self, TypeId) -> bool,
    ) {
        let result = self.u.shallow(result);
        let Some(index) =
            (self.deferred.calls.iter()).position(|p| self.u.shallow(p.result) == result)
        else {
            return;
        };
        let mut p = self.deferred.calls.remove(index);
        let kept: Vec<_> = (p.fitting.iter())
            .filter(|(.., sig, _)| keep(self, sig.ret))
            .cloned()
            .collect();
        if kept.len() != 1 {
            self.deferred.calls.insert(index, p);
            return;
        }
        let (def, func, sig, type_args) = kept.into_iter().next().expect("exactly one");
        p.fitting.clear();
        let _ = self.u.unify(p.result, sig.ret);
        self.finish_call(p.call, def, func, &sig, type_args, &p.args, &p.found);
    }

    /// Decides the calls more than one function fitted, now that more is known about their
    /// results: `let a = make(); a + 1` takes the `make` giving a number. One decided can tell
    /// another's result, so until nothing changes; what's still undecided is reported.
    pub(super) fn resolve_pending_calls(&mut self) {
        let mut pending = std::mem::take(&mut self.deferred.calls);
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

    /// Records that `call` calls `func` (`def`) and checks its arguments, of types `found`.
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
        self.records.callees.insert(call, def);
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
    pub(super) fn pick_function(
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
        self.records.callees.insert(expr, def);
        Some(f)
    }

    /// Reports functions sharing a name that can't be told apart: neither generic, and taking
    /// and giving the same types.
    pub(super) fn check_overloads(&mut self) {
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
                        ) = (self.u.shallow_ty(x), self.u.shallow_ty(y))
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
    pub(super) fn check_args_from(
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
            self.infer_all(args);
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

    /// The span from `method`'s name to the `)` of a call given nothing, which a property
    /// goes in place of. `None` when it's given something.
    pub(super) fn bare_call(
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
}
