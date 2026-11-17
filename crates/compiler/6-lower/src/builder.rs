use som_ast::{Expr, Func, Id, Span};
use som_lang::LangFunc;
use som_mir::*;
use som_resolve::DefKind;
use std::collections::{HashMap, HashSet};

use super::{FnBuilder, Lowerer, Owner, Subst};

impl<'l, 'a> FnBuilder<'l, 'a> {
    pub(super) fn new(l: &'l mut Lowerer<'a>, func: Id<Func>, type_args: Vec<TypeId>) -> Self {
        let subst = Subst::new(l.names.type_params(func), &type_args, &l.typed.impls);
        let span = l.ast.funcs.span(func).clone();
        Self::with(l, Some(func), Some((func, type_args)), span, subst)
    }

    pub(super) fn for_init(l: &'l mut Lowerer<'a>, c: Id<som_ast::Const>, owner: Owner) -> Self {
        let subst = l.owner_subst(&owner);
        let span = l.ast.consts.span(c).clone();
        Self::with(l, None, owner, span, subst)
    }

    pub(super) fn with(
        l: &'l mut Lowerer<'a>,
        func: Option<Id<Func>>,
        owner: Owner,
        span: Span,
        subst: Subst<'a>,
    ) -> Self {
        let typed = l.typed_for(&owner);
        let mut builder = FnBuilder {
            l,
            func,
            owner,
            typed,
            span,
            subst,
            locals: Arena::new(),
            blocks: Arena::new(),
            terminated: HashSet::new(),
            current: Id::new(0),
            bindings: HashMap::new(),
            scopes: Vec::new(),
            loops: Vec::new(),
            reads: Vec::new(),
            two_phase: Vec::new(),
        };
        builder.current = builder.new_block();
        builder
    }

    pub(super) fn param_count(&self) -> usize {
        self.func
            .map_or(0, |func| self.l.ast.funcs[func].params.len())
    }

    /// `_0 = value; return`.
    pub(super) fn build_init(mut self, c: Id<som_ast::Const>) -> Body {
        let value = self.l.ast.consts[c].value;
        let span = self.span.clone();
        let ret = self.expr_ty(value);
        self.new_local(ret, None, false, span.clone());
        let value = self.operand(value);
        self.assign(Place::local(RETURN), Rvalue::Use(value), span.clone());
        self.terminate(TerminatorKind::Return, span);
        self.take_body(0, Vec::new())
    }

    pub(super) fn build(mut self) -> Body {
        let ast = self.l.ast;
        let this = self.func.expect("a function's body");
        let func = &ast.funcs[this];
        let span = self.span.clone();
        self.l.at = Some(span.clone());
        let sig = self.l.typed.func(this);

        let ret = self.ty(sig.ret);
        self.new_local(ret, None, false, span.clone());
        self.bind_params(this, sig.params.clone());

        let body = func.body.expect("only functions with bodies are lowered");
        let value = self.operand(body);
        // One with no final value that should return one never gets to its end: every path
        // returns, as the type checker made sure.
        let unit = self.l.mir.types.unit;
        let reaches_end =
            !matches!(ast.exprs[body], Expr::Block { value: None, .. }) || ret == unit;
        if reaches_end {
            self.assign(Place::local(RETURN), Rvalue::Use(value), span.clone());
            for arg in (1..=func.params.len()).rev() {
                self.end_local(Id::new(arg), span.clone());
            }
            self.terminate(TerminatorKind::Return, span.clone());
        } else {
            self.terminate(TerminatorKind::Unreachable, span.clone());
        }

        let outs = (func.params.iter().enumerate())
            .filter(|(_, p)| p.out)
            .map(|(i, _)| Id::new(i + 1))
            .collect();
        self.take_body(func.params.len(), outs)
    }

    fn bind_params(&mut self, func: Id<Func>, types: Vec<TypeId>) {
        let ast = self.l.ast;
        let params = ast.funcs[func].params.iter();
        for ((param, &def), ty) in params.zip(self.l.names.params(func)).zip(types) {
            let ty = self.ty(ty);
            let local = self.new_local(
                ty,
                Some(param.name.name.to_string()),
                param.mutable || param.out,
                param.name.span.clone(),
            );
            self.bindings.insert(def, local);
        }
    }

    fn take_body(&mut self, arg_count: usize, outs: Vec<Local>) -> Body {
        let mut body = Body {
            locals: std::mem::take(&mut self.locals),
            arg_count,
            outs,
            two_phase: std::mem::take(&mut self.two_phase),
            blocks: std::mem::take(&mut self.blocks),
        };
        self.elaborate_drops(&mut body);
        body
    }

    pub(super) fn ty(&mut self, ty: TypeId) -> TypeId {
        let ty = self.subst.apply(&mut self.l.mir.types, ty);
        // In generic code with stand-ins, a generic struct's fields can be reached before
        // anything else would have made its copy.
        self.l.make_structs_in(ty);
        ty
    }

    pub(super) fn expr_ty(&mut self, expr: Id<Expr>) -> TypeId {
        let ty = self.typed.expr(expr);
        self.ty(ty)
    }

    pub(super) fn span(&self, expr: Id<Expr>) -> Span {
        self.l.ast.exprs.span(expr).clone()
    }

    pub(super) fn new_local(
        &mut self,
        ty: TypeId,
        name: Option<String>,
        mutable: bool,
        span: Span,
    ) -> Local {
        self.locals.alloc(LocalDecl {
            ty,
            name,
            mutable,
            span,
        })
    }

    pub(super) fn temp(&mut self, ty: TypeId, span: Span) -> Local {
        self.new_local(ty, None, false, span)
    }

    pub(super) fn new_block(&mut self) -> BlockId {
        // Blocks nothing jumps to, like the one after an endless `loop`, end here.
        let span = self.span.clone();
        self.blocks.alloc(Block {
            stmts: Vec::new(),
            terminator: Terminator {
                kind: TerminatorKind::Unreachable,
                span,
            },
        })
    }

    pub(super) fn push(&mut self, kind: StatementKind, span: Span) {
        let mut reads = Vec::new();
        if let StatementKind::Assign(_, rvalue) = &kind {
            // The latest pending read of each operand's place: an earlier one belongs to an
            // expression already finished. Backwards, so `f(a, a)` gives each its own.
            for op in rvalue.operands().into_iter().rev() {
                if let Operand::Copy(place) | Operand::Move(place) = op
                    && let Some(i) = self.reads.iter().rposition(|(read, _)| read == place)
                {
                    reads.push(self.reads.remove(i));
                }
            }
            reads.reverse();
        }
        self.blocks[self.current]
            .stmts
            .push(Statement { kind, span, reads });
    }

    /// Notes where the source reads `place`, for the statement using it. A `Share` value is
    /// read as a copy from its `share`.
    pub(super) fn read_at(&mut self, place: Place, ty: TypeId, span: Span) -> Operand {
        if !self.l.is_copy(ty)
            && let Some(share) = self.l.share_impl(ty)
        {
            return self.share(place, ty, share, span);
        }
        // A `T?` of a `Share` `T`: shared by the standard library's `share_optional`.
        if let Ty::Optional(inner) = *self.l.mir.types.get(ty)
            && !self.l.is_copy(inner)
            && self.l.share_impl(inner).is_some()
            && let Some(def) = self.l.names.lang_func(LangFunc::ShareOptional)
            && let DefKind::Func(func) = self.l.names.def(def).kind
        {
            let share = self.l.instance(func, vec![inner]);
            return self.share(place, ty, share, span);
        }
        self.reads.push((place.clone(), span));
        self.read(place, ty)
    }

    /// `share(&place)`.
    pub(super) fn share(&mut self, place: Place, ty: TypeId, share: FuncId, span: Span) -> Operand {
        let borrowed = self.borrow_temp(place, false, ty, span.clone());
        let copy = self.temp(ty, span.clone());
        let call = Rvalue::Call {
            func: share,
            args: vec![Operand::Move(Place::local(borrowed))],
        };
        self.assign(Place::local(copy), call, span.clone());
        self.push(StatementKind::StorageDead(borrowed), span);
        Operand::Move(Place::local(copy))
    }

    pub(super) fn assign(&mut self, place: Place, rvalue: Rvalue, span: Span) {
        self.push(StatementKind::Assign(place, rvalue), span);
    }

    pub(super) fn terminate(&mut self, kind: TerminatorKind, span: Span) {
        let first = self.terminated.insert(self.current);
        debug_assert!(first, "block terminated twice");
        self.blocks[self.current].terminator = Terminator { kind, span };
    }

    pub(super) fn read(&self, place: Place, ty: TypeId) -> Operand {
        if self.l.is_copy(ty) {
            Operand::Copy(place)
        } else {
            Operand::Move(place)
        }
    }

    pub(super) fn constant(&self, kind: ConstKind, ty: TypeId) -> Operand {
        Operand::Const(Const { kind, ty })
    }

    pub(super) fn in_scope<T>(&mut self, span: Span, f: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push(Vec::new());
        let result = f(self);
        let locals = self.scopes.pop().expect("scope was pushed");
        for local in locals.into_iter().rev() {
            self.end_local(local, span.clone());
        }
        result
    }

    pub(super) fn type_args(&mut self, expr: Id<Expr>) -> Vec<TypeId> {
        let args = self.typed.type_args(expr).to_vec();
        args.into_iter().map(|t| self.ty(t)).collect()
    }

    /// With this instance's type arguments filled in.
    pub(super) fn value_of(&mut self, ty: TypeId) -> TypeId {
        self.subst.apply(&mut self.l.mir.types, ty)
    }

    /// `&place` or `&mut place`, of a `to`, in a new temporary.
    pub(super) fn borrow_temp(
        &mut self,
        place: Place,
        mutable: bool,
        to: TypeId,
        span: Span,
    ) -> Local {
        let reference = self.l.mir.types.intern(Ty::Ref { mutable, to });
        let temp = self.temp(reference, span.clone());
        self.assign(Place::local(temp), Rvalue::Ref { mutable, place }, span);
        temp
    }

    /// `rvalue` in a new temporary, read.
    pub(super) fn temp_read(&mut self, ty: TypeId, rvalue: Rvalue, span: Span) -> Operand {
        let temp = self.temp(ty, span.clone());
        self.assign(Place::local(temp), rvalue, span);
        self.read(Place::local(temp), ty)
    }

    /// `rvalue` in a new temporary, copied.
    pub(super) fn temp_copy(&mut self, ty: TypeId, rvalue: Rvalue, span: Span) -> Operand {
        let temp = self.temp(ty, span.clone());
        self.assign(Place::local(temp), rvalue, span);
        Operand::Copy(Place::local(temp))
    }

    /// What value parameter `def` stands for in this instance.
    pub(super) fn value_param(&mut self, def: DefId) -> Value {
        let param = self.l.mir.types.intern(Ty::Param(def));
        let value = self.value_of(param);
        match *self.l.mir.types.get(value) {
            Ty::Value(value) => value,
            _ => unreachable!("value parameters stand for values"),
        }
    }

    pub(super) fn body_local_ty(&self, local: Local) -> TypeId {
        self.locals[local].ty
    }
}
