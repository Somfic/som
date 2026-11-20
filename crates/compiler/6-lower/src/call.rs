use som_ast::{Expr, Func, Id, Span};
use som_mir::*;
use som_resolve::DefKind;
use som_typeck::Receiver;

use super::FnBuilder;

/// The standard library's `Rc` method lending what it shares.
const RC_GET: &str = "get";

impl FnBuilder<'_, '_> {
    pub(super) fn call(
        &mut self,
        expr: Id<Expr>,
        func: Id<Func>,
        receiver: Option<Id<Expr>>,
        args: &[Id<Expr>],
    ) -> Rvalue {
        let type_args = self.type_args(expr);
        let func = self.l.instance(func, type_args);
        let receiver = receiver.map(|r| self.receiver(r));
        let args: Vec<(Operand, TypeId, Span)> = receiver
            .into_iter()
            .chain(
                args.iter()
                    .map(|&a| (self.operand(a), self.expr_ty(a), self.span(a))),
            )
            .collect();
        // Every argument is evaluated before any is reborrowed, so `x.f(x.g())` works.
        let args = args
            .into_iter()
            .map(|(arg, ty, span)| self.reborrow(arg, ty, span))
            .collect();
        Rvalue::Call { func, args }
    }

    /// `callee(args)`, of what's at place `callee`: a call of the function its type names, if
    /// it's a function's own type; of a closure's body, given the closure; or else through
    /// the address it holds.
    pub(super) fn call_value(&mut self, callee: Place, call: Id<Expr>, args: &[Id<Expr>]) -> Rvalue {
        let callee_ty = self.place_ty(&callee);
        let span = self.span(call);
        // Lent before the arguments are worked out, as a method's `self` is.
        let closure = match self.l.closure_instance(callee_ty) {
            Some(func) => {
                let lent = self.borrow_temp(callee.clone(), false, callee_ty, span);
                let lent_ty = self.body_local_ty(lent);
                Some((func, self.read(Place::local(lent), lent_ty)))
            }
            None => None,
        };
        let given: Vec<(Operand, TypeId, Span)> = (args.iter())
            .map(|&a| (self.operand(a), self.expr_ty(a), self.span(a)))
            .collect();
        let mut args: Vec<Operand> = (given.into_iter())
            .map(|(arg, ty, span)| self.reborrow(arg, ty, span))
            .collect();
        if let Some((func, closure)) = closure {
            args.insert(0, closure);
            return Rvalue::Call { func, args };
        }
        match self.l.fn_item_instance(callee_ty) {
            Some(func) => Rvalue::Call { func, args },
            None => Rvalue::CallIndirect {
                callee: Operand::Copy(callee),
                args,
            },
        }
    }

    /// As the method's `self`: as it is, borrowed (`a.len()` for `self: &List`), or copied
    /// out of the reference it is.
    pub(super) fn receiver(&mut self, receiver: Id<Expr>) -> (Operand, TypeId, Span) {
        let span = self.span(receiver);
        // `node.total()` of a `#Node`: called on the `&Node` its `get` lends.
        if self.typed.is_shared_read(receiver) {
            let lent = self.shared_get(receiver);
            let lent_ty = self.body_local_ty(lent);
            let Ty::Ref { to, .. } = *self.l.mir.types.get(lent_ty) else {
                unreachable!("`get` lends a reference")
            };
            return match self.typed.receiver(receiver) {
                // Taking `self` by value: a copy of what it shares.
                Some(Receiver::Deref) => {
                    let value = Place::local(lent).project(Projection::Deref);
                    (self.read(value, to), to, span)
                }
                _ => (Operand::Copy(Place::local(lent)), lent_ty, span),
            };
        }
        if self.typed.is_auto_iter(receiver) {
            return self.auto_iter(receiver);
        }
        let ty = self.expr_ty(receiver);
        match self.typed.receiver(receiver) {
            None => (self.operand(receiver), ty, span),
            Some(Receiver::Borrow { mutable }) => {
                let place = self.place(receiver);
                let temp = self.borrow_temp(place, mutable, ty, span.clone());
                // `v.push(v.len())`: the arguments are evaluated before the call uses it.
                if mutable {
                    self.two_phase.push(temp);
                }
                let reference = self.body_local_ty(temp);
                (self.read(Place::local(temp), reference), reference, span)
            }
            Some(Receiver::Share) => {
                let place = self.place(receiver).project(Projection::Deref);
                let Ty::Ref { to, .. } = *self.l.mir.types.get(ty) else {
                    unreachable!("checked: only references are lent on")
                };
                let temp = self.borrow_temp(place, false, to, span.clone());
                let reference = self.body_local_ty(temp);
                (self.read(Place::local(temp), reference), reference, span)
            }
            Some(Receiver::Deref) => {
                let place = self.place(receiver).project(Projection::Deref);
                let Ty::Ref { to, .. } = *self.l.mir.types.get(ty) else {
                    unreachable!("checked: only references are dereferenced")
                };
                (self.read(place, to), to, span)
            }
        }
    }

    /// `numbers` in `numbers.where(…)`: what `numbers.iter()` makes, which the method takes.
    fn auto_iter(&mut self, receiver: Id<Expr>) -> (Operand, TypeId, Span) {
        let span = self.span(receiver);
        let ty = self.expr_ty(receiver);
        let (lent, viewed) = match *self.l.mir.types.get(ty) {
            Ty::Ref { mutable: false, to } => (self.operand(receiver), to),
            // A `&mut` lent on as a `&`: `&*numbers`.
            Ty::Ref { mutable: true, to } => {
                let place = self.place(receiver).project(Projection::Deref);
                let temp = self.borrow_temp(place, false, to, span.clone());
                (Operand::Move(Place::local(temp)), to)
            }
            _ => {
                let place = self.place(receiver);
                let temp = self.borrow_temp(place, false, ty, span.clone());
                (Operand::Move(Place::local(temp)), ty)
            }
        };
        let iter = self.l.lang_method(som_lang::LangTrait::IterRef);
        let func = self.l.instance(iter, vec![viewed]);
        let made = self.l.mir.funcs[func].ret;
        let call = Rvalue::Call {
            func,
            args: vec![lent],
        };
        let value = self.temp_read(made, call, span.clone());
        (value, made, span)
    }

    /// Passes a `&mut` variable on as `&mut *x` rather than moving it, so it's usable again
    /// after the call. The new loan is what stops two arguments from being the same `&mut`:
    /// codegen marks `&mut` parameters `restrict`, which relies on that.
    pub(super) fn reborrow(&mut self, arg: Operand, ty: TypeId, span: Span) -> Operand {
        let Operand::Move(place) = arg else {
            return arg;
        };
        let fresh = place.projection.is_empty() && self.locals[place.local].name.is_none();
        if fresh || !matches!(self.l.mir.types.get(ty), Ty::Ref { mutable: true, .. }) {
            return Operand::Move(place);
        }
        let temp = self.temp(ty, span.clone());
        let place = place.project(Projection::Deref);
        let borrow = Rvalue::Ref {
            mutable: true,
            place,
        };
        self.assign(Place::local(temp), borrow, span);
        Operand::Move(Place::local(temp))
    }

    /// The `&T` that `#T` `expr`'s `get` lends, in a temporary.
    pub(super) fn shared_get(&mut self, expr: Id<Expr>) -> Local {
        let span = self.span(expr);
        let rc_ty = self.expr_ty(expr);
        let Ty::Struct(rc, ref args) = *self.l.mir.types.get(rc_ty) else {
            unreachable!("checked: only a `#T` is read through")
        };
        let inner = args[0];
        let get = (self.l.names.method(rc, RC_GET)).expect("the standard library's `Rc` has `get`");
        let DefKind::Func(get) = self.l.names.def(get).kind else {
            unreachable!("methods are functions")
        };
        let func = self.l.instance(get, vec![inner]);
        let place = self.place_of(expr);
        let borrowed = self.borrow_temp(place, false, rc_ty, span.clone());
        let lent_ty = self.l.mir.types.intern(Ty::Ref {
            mutable: false,
            to: inner,
        });
        let lent = self.temp(lent_ty, span.clone());
        let call = Rvalue::Call {
            func,
            args: vec![Operand::Move(Place::local(borrowed))],
        };
        self.assign(Place::local(lent), call, span);
        lent
    }
}
