//! `T!`, the standard library's `Outcome<T>`: `Ok(T)`, or `Failed(Error)`. `fail value`
//! makes an `Error` of `value` and returns it, `value?` returns the failure it holds, and
//! `else` gives something in its place.

use som_ast::{Expr, Id, Pattern, Span};
use som_lang::{LangFunc, LangStruct};
use som_mir::*;
use som_resolve::DefKind;

use super::FnBuilder;

/// Outcome's variants, in the order the standard library declares them.
pub(super) const OK: usize = 0;
pub(super) const FAILED: usize = 1;

/// The field of the standard library's `Error` holding what it failed with.
const ERROR_VALUE: &str = "what";

impl FnBuilder<'_, '_> {
    /// `T`, of a `T!`.
    pub(super) fn outcome_inner(&self, ty: TypeId) -> Option<TypeId> {
        let outcome = self.l.names.lang_struct(LangStruct::Outcome)?;
        match self.l.mir.types.get(ty) {
            Ty::Struct(def, args) if *def == outcome && args.len() == 1 => Some(args[0]),
            _ => None,
        }
    }

    /// `T!` holding `ok`.
    pub(super) fn outcome_of(&mut self, ok: TypeId) -> TypeId {
        let outcome = self.l.names.lang_struct(LangStruct::Outcome).expect("checked");
        self.l.mir.types.intern(Ty::Struct(outcome, vec![ok]))
    }

    /// The standard library's `Error`.
    pub(super) fn error_ty(&mut self) -> TypeId {
        let error = self.l.names.lang_struct(LangStruct::Error).expect("checked");
        self.l.mir.types.intern(Ty::Struct(error, Vec::new()))
    }

    /// Where `variant`'s one field is, in a `T!` of type `ty`.
    pub(super) fn outcome_field(&self, ty: TypeId, variant: usize) -> usize {
        let Ty::Struct(def, args) = self.l.mir.types.get(ty).clone() else {
            unreachable!("a `T!` is a struct")
        };
        self.l.mir.struct_def(def, &args).variant_fields(variant)[0]
    }

    /// Calls the standard library's `func`, with `type_args`, giving a `ret`.
    fn call_lang(
        &mut self,
        func: LangFunc,
        type_args: Vec<TypeId>,
        args: Vec<Operand>,
        ret: TypeId,
        span: &Span,
    ) -> Local {
        let def = self.l.names.lang_func(func).expect("the standard library has it");
        let DefKind::Func(f) = self.l.names.def(def).kind else {
            unreachable!("a function")
        };
        let func = self.l.instance(f, type_args);
        let result = self.temp(ret, span.clone());
        self.assign(Place::local(result), Rvalue::Call { func, args }, span.clone());
        result
    }

    /// `value`, of type `ok`, as the `T!` it's wrapped in.
    pub(super) fn ok_wrapped(&mut self, value: Operand, ok: TypeId) -> Rvalue {
        let ty = self.outcome_of(ok);
        Rvalue::Struct {
            ty,
            variant: Some(OK),
            fields: vec![value],
        }
    }

    /// Returns a failure of `error`, an `Error`, from the function.
    fn return_failure(&mut self, error: Operand, span: Span) {
        let ret = self.locals[RETURN].ty;
        let failed = Rvalue::Struct {
            ty: ret,
            variant: Some(FAILED),
            fields: vec![error],
        };
        self.assign(Place::local(RETURN), failed, span.clone());
        self.end_scopes_from(0, span.clone());
        for arg in (1..=self.param_count()).rev() {
            self.end_local(Id::new(arg), span.clone());
        }
        self.terminate(TerminatorKind::Return, span);
        self.current = self.new_block();
    }

    /// `fail value`. In the `else` of a failure that isn't given a name, that failure is
    /// kept: text says what was being done when it happened, and anything else fails in its
    /// place, with it as why.
    pub(super) fn raise(&mut self, expr: Id<Expr>, value: Id<Expr>) -> Rvalue {
        let span = self.span(expr);
        let given = self.used_ty(value);
        let operand = self.operand(value);
        let error_ty = self.error_ty();
        let text = (self.l.names.lang_struct(LangStruct::String))
            .map(|s| self.l.mir.types.intern(Ty::Struct(s, Vec::new())));
        let error = if given == error_ty {
            operand
        } else {
            let made = match self.handling.last().copied() {
                Some(cause) if Some(given) == text => {
                    let cause = Operand::Move(Place::local(cause));
                    self.call_lang(LangFunc::ErrorContext, Vec::new(), vec![cause, operand], error_ty, &span)
                }
                Some(cause) => {
                    let cause = Operand::Move(Place::local(cause));
                    self.call_lang(LangFunc::ErrorCaused, vec![given], vec![cause, operand], error_ty, &span)
                }
                None => self.call_lang(LangFunc::ErrorOf, vec![given], vec![operand], error_ty, &span),
            };
            Operand::Move(Place::local(made))
        };
        self.return_failure(error, span);
        let ty = self.expr_ty(expr);
        Rvalue::Use(self.constant(ConstKind::Zeroed, ty))
    }

    /// Branches on whether the `T!` at `place`, of type `ty`, holds a value: in the first
    /// block it does, in the second it failed.
    fn branch_on_ok(&mut self, place: &Place, span: Span) -> (BlockId, BlockId) {
        let bool = self.l.mir.types.bool;
        let tag = place.clone().project(Projection::Field(0));
        let tag_ty = self.place_ty(&tag);
        let ok = self.constant(ConstKind::Int(OK as i128), tag_ty);
        let is_ok = Rvalue::Binary {
            op: BinOp::Equals,
            lhs: Operand::Copy(tag),
            rhs: ok,
        };
        let condition = self.temp_copy(bool, is_ok, span.clone());
        let (ok_block, failed_block) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then: ok_block,
                otherwise: failed_block,
            },
            span,
        );
        (ok_block, failed_block)
    }

    /// `value?`: the `T` it holds, or the function fails with its failure.
    pub(super) fn propagate(&mut self, value: Id<Expr>, ty: TypeId, span: Span) -> Operand {
        let outcome = self.expr_ty(value);
        let place = self.place(value);
        let (ok_block, failed_block) = self.branch_on_ok(&place, span.clone());
        self.current = failed_block;
        let failed = self.outcome_field(outcome, FAILED);
        let error = Operand::Move(place.clone().project(Projection::Field(failed)));
        self.return_failure(error, span);
        self.current = ok_block;
        let ok = self.outcome_field(outcome, OK);
        self.read(place.project(Projection::Field(ok)), ty)
    }

    /// `value else fallback`, `value else e => fallback` with `error` the pattern naming
    /// why: the `T` it holds, or `fallback`. Without a name, a `fail` in `fallback` keeps the
    /// failure.
    pub(super) fn failable_fallback(
        &mut self,
        value: Id<Expr>,
        error: Option<Id<Pattern>>,
        fallback: Id<Expr>,
        ty: TypeId,
        span: Span,
    ) -> Operand {
        let outcome = self.expr_ty(value);
        let result = self.temp(ty, span.clone());
        let place = self.place(value);
        let (ok_block, failed_block) = self.branch_on_ok(&place, span.clone());
        let join = self.new_block();

        self.current = ok_block;
        let ok = self.outcome_field(outcome, OK);
        let held = self.read(place.clone().project(Projection::Field(ok)), ty);
        self.assign(Place::local(result), Rvalue::Use(held), span.clone());
        self.terminate(TerminatorKind::Goto(join), span.clone());

        self.current = failed_block;
        let failed = self.outcome_field(outcome, FAILED);
        let error_ty = self.error_ty();
        let diverges = self.typed.diverges(fallback);
        self.in_scope(span.clone(), |b| {
            let name = error.and_then(|e| match &b.l.ast.patterns[e] {
                Pattern::Binding { name, .. } => Some(name.name.to_string()),
                _ => None,
            });
            let why = b.new_local(error_ty, name, false, span.clone());
            b.declare(why, span.clone());
            let taken = Operand::Move(place.project(Projection::Field(failed)));
            b.assign(Place::local(why), Rvalue::Use(taken), span.clone());
            match error.and_then(|e| b.l.names.pattern(e)) {
                Some(def) => {
                    b.bindings.insert(def, why);
                }
                None => b.handling.push(why),
            }
            let value = b.operand(fallback);
            if error.is_none() {
                b.handling.pop();
            }
            if !diverges {
                b.assign(Place::local(result), Rvalue::Use(value), span.clone());
            }
        });
        let after = match diverges {
            true => TerminatorKind::Unreachable,
            false => TerminatorKind::Goto(join),
        };
        self.terminate(after, span.clone());

        self.current = join;
        self.read(Place::local(result), ty)
    }

    /// Tests that the `Error` at `place` holds a value of struct or enum `def`, branching to
    /// `fail` if not, and gives where it is, lent.
    pub(super) fn failed_with(
        &mut self,
        place: Place,
        def: som_resolve::DefId,
        fail: BlockId,
        span: Span,
    ) -> (Place, TypeId) {
        let error_ty = self.error_ty();
        let Ty::Struct(error, _) = *self.l.mir.types.get(error_ty) else {
            unreachable!("`Error` is a struct")
        };
        // Its `dyn Display`, holding the value.
        let what = (self.l.mir.struct_def(error, &[]).fields.iter())
            .position(|(name, _)| name == ERROR_VALUE)
            .expect("the standard library's `Error` holds its value");
        let kind = self.l.mir.types.intern(Ty::Struct(def, Vec::new()));
        let held = self.dyn_holds(place.project(Projection::Field(what)), kind, fail, span);
        (held, kind)
    }
}
