//! Closures: in MIR, each is a struct holding its captures, copied if they copy and lent
//! otherwise, and a function for its body, taking the struct lent and then its parameters.

use som_ast::{Expr, Id};
use som_mir::*;
use som_resolve::{DefId, DefKind};
use som_typeck::ClosureSig;

use super::instances::Subst;
use super::{FnBuilder, Job, Lowerer, Owner};

impl<'a> Lowerer<'a> {
    /// What closure `closure` takes, gives and captures, as checked.
    fn closure_sig(&self, closure: Id<Expr>) -> ClosureSig {
        (self.typed.closure(closure))
            .expect("checked closures are recorded")
            .clone()
    }

    /// The function closure `closure` is in, with `args` for its type parameters.
    fn closure_owner(&self, closure: Id<Expr>, args: &[TypeId]) -> Owner {
        let owner = self.closure_sig(closure).owner?;
        (!args.is_empty()).then(|| (owner, args.to_vec()))
    }

    fn closure_subst(&self, closure: Id<Expr>, args: &[TypeId]) -> Subst<'a> {
        let owner = self.closure_owner(closure, args);
        self.owner_subst(&owner)
    }

    /// The struct closure `def` is, with `args` for its function's type parameters: a field
    /// for each capture, its value if it copies and else a reference to it.
    pub(super) fn closure_struct(&mut self, def: DefId, args: Vec<TypeId>) {
        let DefKind::Closure(closure) = self.names.def(def).kind else {
            unreachable!("a closure's struct")
        };
        let sig = self.closure_sig(closure);
        let subst = self.closure_subst(closure, &args);
        let mut fields = Vec::new();
        for (captured, ty) in sig.captures {
            let ty = subst.apply(&mut self.mir.types, ty);
            self.make_structs_in(ty);
            let ty = match self.is_copy(ty) {
                true => ty,
                false => self.mir.types.intern(Ty::Ref {
                    mutable: false,
                    to: ty,
                }),
            };
            fields.push((self.names.def(captured).name.to_string(), ty));
        }
        self.mir.add_struct(StructDef {
            def,
            args,
            name: "closure".into(),
            fields,
            variants: None,
            copy: true,
            handle: false,
        });
    }

    /// For a closure's struct type: the function its body is, for its type arguments.
    pub(super) fn closure_instance(&mut self, ty: TypeId) -> Option<FuncId> {
        let Ty::Struct(def, args) = self.mir.types.get(ty).clone() else {
            return None;
        };
        let DefKind::Closure(closure) = self.names.def(def).kind else {
            return None;
        };
        let key = (closure, args.clone());
        if let Some(&id) = self.closure_instances.get(&key) {
            return Some(id);
        }
        let sig = self.closure_sig(closure);
        let subst = self.closure_subst(closure, &args);
        let lent = self.mir.types.intern(Ty::Ref {
            mutable: false,
            to: ty,
        });
        let mut params = vec![lent];
        for &param in &sig.params {
            let param = subst.apply(&mut self.mir.types, param);
            self.make_structs_in(param);
            params.push(param);
        }
        let ret = subst.apply(&mut self.mir.types, sig.ret);
        self.make_structs_in(ret);
        let span = self.ast.exprs.span(closure).clone();
        let id = self.mir.funcs.alloc(Function {
            name: format!("closure_{}", span.start.line),
            params,
            ret,
            span,
            body: None,
            link_name: None,
            library: None,
            is_entry: false,
            variadic: false,
        });
        self.queue.push((id, Job::Closure(closure, args)));
        self.closure_instances.insert(key, id);
        Some(id)
    }
}

impl<'l, 'a> FnBuilder<'l, 'a> {
    pub(super) fn for_closure(l: &'l mut Lowerer<'a>, closure: Id<Expr>, args: Vec<TypeId>) -> Self {
        let owner = l.closure_owner(closure, &args);
        let subst = l.owner_subst(&owner);
        let span = l.ast.exprs.span(closure).clone();
        Self::with(l, None, owner, span, subst)
    }

    /// The closure, lent, then its parameters; what it captures is reached through the
    /// closure.
    pub(super) fn build_closure(mut self, closure: Id<Expr>) -> Body {
        let Expr::Closure { params, body } = &self.l.ast.exprs[closure] else {
            unreachable!("a closure's body")
        };
        let body = *body;
        let span = self.span.clone();
        self.l.at = Some(span.clone());
        let sig = self.l.closure_sig(closure);

        let ret = self.ty(sig.ret);
        self.new_local(ret, None, false, span.clone());
        let def = self.l.names.closure(closure).expect("closures are resolved");
        let args = match &self.owner {
            Some((_, args)) => args.clone(),
            None => Vec::new(),
        };
        let this = self.l.mir.types.intern(Ty::Struct(def, args));
        let lent = self.l.mir.types.intern(Ty::Ref {
            mutable: false,
            to: this,
        });
        let env = self.new_local(lent, None, false, span.clone());
        let fields = self.l.mir.struct_def(def, &self.args_of(this)).fields.clone();
        for (index, ((captured, _), (_, field_ty))) in sig.captures.iter().zip(&fields).enumerate() {
            let mut place = Place::local(env)
                .project(Projection::Deref)
                .project(Projection::Field(index));
            if matches!(self.l.mir.types.get(*field_ty), Ty::Ref { .. }) {
                place = place.project(Projection::Deref);
            }
            self.captures.insert(*captured, place);
        }
        let defs = self.l.names.closure_params(closure).to_vec();
        for ((param, def), &ty) in params.iter().zip(defs).zip(&sig.params) {
            let ty = self.ty(ty);
            let local = self.new_local(
                ty,
                Some(param.name.name.to_string()),
                false,
                param.name.span.clone(),
            );
            self.bindings.insert(def, local);
        }

        let value = self.operand(body);
        let unit = self.l.mir.types.unit;
        let reaches_end =
            !matches!(self.l.ast.exprs[body], Expr::Block { value: None, .. }) || ret == unit;
        if reaches_end {
            self.assign(Place::local(RETURN), Rvalue::Use(value), span.clone());
            for arg in (2..=params.len() + 1).rev() {
                self.end_local(Id::new(arg), span.clone());
            }
            self.terminate(TerminatorKind::Return, span.clone());
        } else {
            self.terminate(TerminatorKind::Unreachable, span.clone());
        }
        self.take_body(params.len() + 1, Vec::new())
    }

    fn args_of(&self, ty: TypeId) -> Vec<TypeId> {
        match self.l.mir.types.get(ty) {
            Ty::Struct(_, args) => args.clone(),
            _ => Vec::new(),
        }
    }

    /// `n => *n > limit`, where it's written: its struct, holding a copy of each capture that
    /// copies and a reference to each other.
    pub(super) fn closure_value(&mut self, closure: Id<Expr>) -> Rvalue {
        let ty = self.expr_ty(closure);
        let Ty::Struct(def, args) = self.l.mir.types.get(ty).clone() else {
            unreachable!("a closure's type is its struct")
        };
        // Its body is made with it, even if nothing calls it.
        self.l.closure_instance(ty);
        let span = self.span(closure);
        let captured = self.l.names.captures(closure).to_vec();
        let fields_tys = self.l.mir.struct_def(def, &args).fields.clone();
        let mut fields = Vec::new();
        for (def, (_, field_ty)) in captured.into_iter().zip(fields_tys) {
            let place = self.variable(def).expect("captures are variables");
            let operand = match *self.l.mir.types.get(field_ty) {
                Ty::Ref { to, .. } => {
                    let lent = self.borrow_temp(place, false, to, span.clone());
                    Operand::Copy(Place::local(lent))
                }
                _ => Operand::Copy(place),
            };
            fields.push(operand);
        }
        Rvalue::Struct {
            ty,
            variant: None,
            fields,
        }
    }
}
