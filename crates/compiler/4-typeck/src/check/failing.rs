//! What can fail: `T!`, which is the standard library's `Outcome<T>`. `fail value` gives up
//! with `value`, `value?` passes a failure on, `value else …` and `value else e => …` give
//! something in its place, and a `match` takes `fail …` arms.

use som_ast::{Expr, Id, Pattern};
use som_lang::LangStruct;
use som_resolve::DefKind;

use super::Checker;
use crate::error::TypeError;
use crate::exhaustive::{Ctor, Pat};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;
use crate::{Failed, PatternInfo};

impl Checker<'_> {
    /// `T`, of a `T!`.
    pub(super) fn outcome_inner(&self, ty: TypeId) -> Option<TypeId> {
        let outcome = self.names.lang_struct(LangStruct::Outcome)?;
        match self.u.shallow_ty(ty) {
            Ty::Struct(def, args) if *def == outcome && args.len() == 1 => Some(args[0]),
            _ => None,
        }
    }

    /// `T!`.
    pub(super) fn outcome_of(&mut self, ok: TypeId) -> TypeId {
        match self.names.lang_struct(LangStruct::Outcome) {
            Some(outcome) => self.u.intern(Ty::Struct(outcome, vec![ok])),
            None => self.u.table.error,
        }
    }

    /// Why a `T!` failed: the standard library's `Error`.
    pub(super) fn error_type(&mut self) -> TypeId {
        match self.names.lang_struct(LangStruct::Error) {
            Some(error) => self.u.intern(Ty::Struct(error, Vec::new())),
            None => self.u.table.error,
        }
    }

    /// What the function `fail` or `?` is in gives, if it can fail: its `T`. A closure's not
    /// said yet becomes one that can.
    fn failing_function(&mut self, at: Id<Expr>, what: &'static str) -> Option<TypeId> {
        let (ret, from) = self.returns.clone().expect("expressions are inside a function");
        if let Some(ok) = self.outcome_inner(ret) {
            return Some(ok);
        }
        if self.u.var_kind(ret) == Some(VarKind::Any) {
            let ok = self.u.fresh(VarKind::Any);
            let outcome = self.outcome_of(ok);
            if self.u.unify(ret, outcome).is_ok() {
                return Some(ok);
            }
        }
        if *self.u.shallow_ty(ret) != Ty::Error {
            self.errors.push(TypeError::FailInInfallible {
                what,
                gives: self.describe(ret),
                span: self.span(at),
                returns: from,
            });
        }
        None
    }

    /// `fail value`: never gives a value, like `@unreachable`. `value` is anything shown as
    /// text, or an `Error` failing again.
    pub(super) fn infer_raise(&mut self, id: Id<Expr>, value: Id<Expr>) -> TypeId {
        let ty = (self.hints.expected_here.take()).unwrap_or_else(|| self.u.fresh(VarKind::Any));
        self.failing_function(id, "fail");
        let given = self.infer(value);
        self.text_as_string(given);
        let error = self.error_type();
        if self.u.shallow(given) != error {
            self.deferred.displayed.push((value, given));
        }
        self.deferred.unreachables.push(ty);
        ty
    }

    /// `value?`: the `T` a `T!` holds.
    pub(super) fn infer_propagate(&mut self, id: Id<Expr>, value: Id<Expr>) -> TypeId {
        let found = self.infer(value);
        let ok = self.failable_inner(found, value, "?");
        // Said already, if it can't fail.
        if *self.u.shallow_ty(ok) != Ty::Error {
            self.failing_function(id, "?");
        }
        ok
    }

    /// The `T` of `found`, a `T!` `expr` gives, for `what` to take apart.
    fn failable_inner(&mut self, found: TypeId, expr: Id<Expr>, what: &'static str) -> TypeId {
        if let Some(ok) = self.outcome_inner(found) {
            return ok;
        }
        let ok = self.u.fresh(VarKind::Any);
        let outcome = self.outcome_of(ok);
        if self.u.unify(found, outcome).is_ok() {
            return ok;
        }
        if *self.u.shallow_ty(found) != Ty::Error {
            self.errors.push(TypeError::NotFailable {
                what,
                ty: self.describe(found),
                optional: matches!(self.u.shallow_ty(found), Ty::Optional(_)),
                span: self.span(expr),
            });
        }
        self.u.table.error
    }

    /// `value else fallback` of a `T!`: the `T`, or `fallback` in its place.
    pub(super) fn infer_failable_fallback(&mut self, failable: Id<Expr>, ok: TypeId, fallback: Id<Expr>) -> TypeId {
        let from = self.span(failable);
        self.expect(fallback, ok, Some(from));
        ok
    }

    /// `value else e => body`: the `T`, or `body`, with `e` the `Error`.
    pub(super) fn infer_recover(
        &mut self,
        failable: Id<Expr>,
        error: Id<Pattern>,
        body: Id<Expr>,
    ) -> TypeId {
        let found = self.infer(failable);
        let ok = self.failable_inner(found, failable, "else e =>");
        let why = self.error_type();
        if let Some(def) = self.names.pattern(error) {
            self.locals.insert(def, why);
        }
        self.records.patterns.insert(error, PatternInfo::default());
        let from = self.span(failable);
        self.expect(body, ok, Some(from));
        ok
    }

    /// `Outcome::Ok(v)`: the standard library's own pattern for what a `T!` is made of.
    pub(super) fn names_outcome_variant(&self, id: Id<Pattern>) -> bool {
        let outcome = self.names.lang_struct(LangStruct::Outcome);
        matches!(
            self.names.pattern(id).map(|d| &self.names.def(d).kind),
            Some(DefKind::Variant { strukt, .. }) if Some(self.names.strukt(*strukt)) == outcome
        )
    }

    /// A pattern matched against a `T!`, `ok` its `T`: `fail …` matches why it failed, `_`
    /// anything, and any other pattern the `T` it holds.
    pub(super) fn outcome_pattern(
        &mut self,
        id: Id<Pattern>,
        ok: TypeId,
        by_ref: Option<bool>,
        none_matched: bool,
    ) -> Pat {
        let ast = self.ast;
        match &ast.patterns[id] {
            Pattern::Wildcard => {
                self.records.patterns.insert(id, PatternInfo::default());
                Pat::Wild
            }
            Pattern::Failed(inner) => {
                let inner = *inner;
                let (failed, pat) = self.failed_pattern(inner, by_ref);
                let info = PatternInfo {
                    failed: Some(failed),
                    ..PatternInfo::default()
                };
                self.records.patterns.insert(id, info);
                Pat::Ctor(Ctor::Variant(1), vec![pat])
            }
            _ => {
                let pat = self.check_pattern(id, ok, by_ref, none_matched);
                let info = self.records.patterns.entry(id).or_default();
                info.unwraps_ok = true;
                Pat::Ctor(Ctor::Variant(0), vec![pat])
            }
        }
    }

    /// What's after `fail` in a pattern: a name for the `Error`, or a pattern for a value of
    /// the type it names, `ParseError::Empty` or `NotFound { path }`.
    fn failed_pattern(&mut self, inner: Id<Pattern>, by_ref: Option<bool>) -> (Failed, Pat) {
        let ast = self.ast;
        let kind = match &ast.patterns[inner] {
            Pattern::Variant { .. } => match self.names.pattern(inner).map(|d| (d, &self.names.def(d).kind)) {
                Some((_, DefKind::Variant { strukt, .. })) => Some(self.names.strukt(*strukt)),
                Some((def, DefKind::Struct(_))) => Some(def),
                _ => None,
            },
            _ => None,
        };
        let Some(def) = kind else {
            // `fail e`, `fail _`: the `Error` itself.
            if !matches!(ast.patterns[inner], Pattern::Wildcard | Pattern::Binding { .. }) {
                self.errors.push(TypeError::FailedPatternNotAType {
                    span: ast.patterns.span(inner).clone(),
                });
                self.bind_all(inner, self.u.table.error);
                return (Failed::Any, Pat::Wild);
            }
            let why = self.error_type();
            self.check_pattern(inner, why, by_ref, false);
            return (Failed::Any, Pat::Wild);
        };
        // Matched through a reference to it, where the `Error` keeps it.
        let ty = self.u.intern(Ty::Struct(def, Vec::new()));
        let pat = self.check_pattern(inner, ty, Some(false), false);
        // Holding the value's pattern, for `FailureArms` to see what of the type it matches.
        (Failed::Of(def), Pat::Ctor(Ctor::Kind(def), vec![pat]))
    }
}
