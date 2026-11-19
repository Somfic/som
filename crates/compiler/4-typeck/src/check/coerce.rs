use som_ast::{Expr, Id, Span};
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::Receiver;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

impl Checker<'_> {
    pub(super) fn expect(&mut self, expr: Id<Expr>, expected: TypeId, expected_from: Option<Span>) {
        self.hints.expected = Some(expected);
        let found = self.infer(expr);
        if self.coerce(expr, expected, found) {
            return;
        }
        // A `string` where C needs a `cstr`: say why they differ.
        if self.u.shallow(expected) == self.u.table.cstr
            && self.u.shallow(found) == self.u.table.text
        {
            self.errors.push(TypeError::StrToC {
                span: self.span(expr),
            });
            return;
        }
        // `println(xs.get(0))` of a list nothing's said the values of yet: which `From`
        // converts it is chosen once it has, after the bodies.
        if self.conversion_undecided(expected, found) {
            self.deferred
                .conversions
                .push((expr, expected, found, expected_from));
            return;
        }
        self.mismatch_at(expr, expected, found, expected_from);
    }

    pub(super) fn coerce(&mut self, expr: Id<Expr>, expected: TypeId, found: TypeId) -> bool {
        // `I::Item` of an `I` that's known by now is what its implementation says.
        let (expected, found) = (self.normalize_assoc(expected), self.normalize_assoc(found));
        if self.u.unify(expected, found).is_ok() {
            return true;
        }
        let expected = self.u.shallow(expected);
        // A `&mut T` where a `&T` is expected: lent as `&*r`, which leaves `r` to use after.
        if let Ty::Ref {
            mutable: false,
            to: wanted,
        } = *self.u.get(expected)
            && let Ty::Ref {
                mutable: true,
                to: given,
            } = *self.u.shallow_ty(found)
            && self.u.unify(wanted, given).is_ok()
        {
            self.records.downgraded.insert(expr);
            return true;
        }
        if self.slices_to(expected, found) {
            self.sliced.insert(expr, expected);
            return true;
        }
        if let Ty::Optional(inner) = *self.u.get(expected) {
            if self.u.unify(inner, found).is_ok() {
                self.records.wrapped.insert(expr);
                return true;
            }
            if self.slices_to(inner, found) {
                self.sliced.insert(expr, inner);
                self.records.wrapped.insert(expr);
                return true;
            }
        }
        // `is_even` where a `fn(&i32) -> bool` is expected: its address.
        if matches!(self.u.get(expected), Ty::Fn { .. })
            && let Ty::FnItem(def, args) = self.u.shallow_ty(found).clone()
            && let Some(signature) = self.item_signature(def, &args)
            && self.u.unify(expected, signature).is_ok()
        {
            self.records.fn_pointers.insert(expr);
            return true;
        }
        if self.from_conversion(expr, expected, found) {
            return true;
        }
        // `&x` of a narrowed `x` borrows the value, but where a reference to the optional is
        // expected, it borrows the optional: `read(&o)` for `fn read(o: &(i32?))`.
        if let Expr::Borrow {
            mutable,
            expr: borrowed,
        } = self.ast.exprs[expr]
            && self.records.narrowed_reads.contains(&borrowed)
        {
            let value = self.exprs[borrowed.id].unwrap_or(self.u.table.error);
            let optional = self.u.intern(Ty::Optional(value));
            let reference = self.u.intern(Ty::Ref {
                mutable,
                to: optional,
            });
            if self.u.unify(expected, reference).is_ok() {
                self.records.narrowed_reads.remove(&borrowed);
                self.exprs[borrowed.id] = Some(optional);
                self.exprs[expr.id] = Some(reference);
                // Through `&mut` of the optional, it can be made `none`.
                if mutable && let Some(borrowed) = self.key(borrowed) {
                    self.forget(|key| key.within(&borrowed));
                }
                return true;
            }
        }
        false
    }

    /// What function `def` takes and gives, with type arguments `args`: `fn(&i32) -> bool`.
    /// `None` if `def` isn't a function.
    pub(super) fn item_signature(&mut self, def: DefId, args: &[TypeId]) -> Option<TypeId> {
        let DefKind::Func(func) = self.names.def(def).kind else {
            return None;
        };
        let subst: Vec<(DefId, TypeId)> =
            (self.names.type_params(func).iter().copied()).zip(args.iter().copied()).collect();
        let sig = &self.sigs[func.id];
        let (params, ret) = (sig.params.clone(), sig.ret);
        let params = params
            .into_iter()
            .map(|p| {
                let p = self.substitute(p, &subst);
                self.normalize_assoc(p)
            })
            .collect();
        let ret = self.substitute(ret, &subst);
        let ret = self.normalize_assoc(ret);
        Some(self.u.intern(Ty::Fn { params, ret }))
    }

    /// Whether a value of type `found` becomes the slice `expected`: a reference to an array of
    /// the same elements (`&numbers` for a `[i32]`, `&mut numbers` for a `[mut i32]`), or a
    /// `[mut T]` where a `[T]` is.
    fn slices_to(&mut self, expected: TypeId, found: TypeId) -> bool {
        let shallow = |c: &Self, ty: TypeId| c.u.shallow_ty(ty).clone();
        let Ty::Ref {
            mutable: wants_mut,
            to: slice,
        } = shallow(self, expected)
        else {
            return false;
        };
        let Ty::Slice(element) = shallow(self, slice) else {
            return false;
        };
        let Ty::Ref {
            mutable: is_mut,
            to: viewed,
        } = shallow(self, found)
        else {
            return false;
        };
        if wants_mut && !is_mut {
            return false;
        }
        // `&b` of a collection nothing has decided yet: borrowing it makes it the array.
        if let Some(VarKind::Collection { element, len }) = self.u.var_kind(viewed) {
            let array = self.u.intern(Ty::Array { element, len });
            let _ = self.u.unify(viewed, array);
        }
        match shallow(self, viewed) {
            Ty::Array { element: found, .. } => self.u.unify(element, found).is_ok(),
            Ty::Slice(found) => is_mut && !wants_mut && self.u.unify(element, found).is_ok(),
            _ => false,
        }
    }

    /// What unwrapping an optional of type `ty` gives; through a reference, `&T?` gives `&T`,
    /// leaving the optional in place.
    pub(super) fn optional_inner(&mut self, ty: TypeId, expr: Id<Expr>) -> TypeId {
        let shallow = self.u.shallow(ty);
        if let Ty::Ref { mutable, to } = *self.u.get(shallow)
            && let Ty::Optional(inner) = *self.u.shallow_ty(to)
        {
            return self.u.intern(Ty::Ref { mutable, to: inner });
        }
        let inner = self.u.fresh(VarKind::Any);
        let optional = self.u.intern(Ty::Optional(inner));
        if self.u.unify(optional, ty).is_err() {
            self.errors.push(TypeError::NotOptional {
                ty: self.describe(ty),
                span: self.span(expr),
            });
            return self.u.table.error;
        }
        inner
    }

    /// Makes method call receiver `receiver`, of type `found`, the `self` of a method taking
    /// `expected`, returning whether it can be: as it is; borrowed, when the method takes a
    /// reference to what it is (`a.len()` for `self: &List`); or copied out of a reference to
    /// what the method takes, if that copies.
    pub(super) fn receive(&mut self, receiver: Id<Expr>, expected: TypeId, found: TypeId) -> bool {
        let wanted = self.u.shallow_ty(expected).clone();
        let given = self.u.shallow_ty(found).clone();
        let adjusted = match (wanted, given) {
            (Ty::Ref { mutable, to }, given) if !matches!(given, Ty::Ref { .. } | Ty::Error) => {
                if self.u.unify(to, found).is_err() {
                    return false;
                }
                // Like `&mut a`: it can change anything inside `a`, and `a` has to allow that.
                if mutable {
                    if let Some(borrowed) = self.key(receiver) {
                        self.forget(|key| key.within(&borrowed) && !key.same(&borrowed));
                    }
                    self.check_mutable(receiver, true);
                }
                Receiver::Borrow { mutable }
            }
            // `self.get()` with `&mut self`, for `fn get(&self)`: lent as `&`.
            (
                Ty::Ref {
                    mutable: false,
                    to: wanted,
                },
                Ty::Ref {
                    mutable: true,
                    to: given,
                },
            ) => {
                if self.u.unify(wanted, given).is_err() {
                    return false;
                }
                Receiver::Share
            }
            (wanted, Ty::Ref { to, .. }) if !matches!(wanted, Ty::Ref { .. } | Ty::Var(_)) => {
                if self.u.unify(expected, to).is_err() {
                    return false;
                }
                // Taking it would move it out from behind the reference.
                if !self.is_copy(to) {
                    self.errors.push(TypeError::ReceiverBehindReference {
                        ty: self.describe(to),
                        span: self.span(receiver),
                    });
                    return true;
                }
                Receiver::Deref
            }
            _ => return self.u.unify(expected, found).is_ok(),
        };
        self.records.receivers.insert(receiver, adjusted);
        true
    }

    /// Whether values of `ty` are copied rather than moved. A type that isn't known yet is
    /// taken to be.
    pub(super) fn is_copy(&self, ty: TypeId) -> bool {
        match *self.u.shallow_ty(ty) {
            Ty::Struct(def, _) => match self.names.def(def).kind {
                DefKind::Struct(s) => self.ast.structs[s].copy || self.implements_copy(def),
                _ => true,
            },
            Ty::Ref { mutable: true, .. } => false,
            Ty::Optional(inner) | Ty::Array { element: inner, .. } => self.is_copy(inner),
            // Whatever it stands for might move: only copied if it's required to be `Copy`, or
            // called, as only functions and their addresses are.
            Ty::Param(def) => {
                self.signatures.contains_key(&def)
                    || (self.names.lang(som_lang::LangTrait::Copy))
                        .is_some_and(|copy| self.names.bounds(def).contains(&copy))
            }
            _ => true,
        }
    }

    /// Makes `ty`, text in quotes nothing's decided yet, the standard library's `string`,
    /// whose methods and fields it then has. Without it loaded, text stays undecided.
    pub(super) fn text_as_string(&mut self, ty: TypeId) {
        let text = self.u.table.text;
        if self.u.var_kind(ty) == Some(VarKind::Text) && text != self.u.table.str_ref {
            let _ = self.u.unify(ty, text);
        }
    }

    /// Settles the numbers in `ty` that nothing has decided yet to what they'd end up as
    /// anyway, `i32` and `f64`. For choosing between impl blocks that an undecided
    /// `[1.0, 2.0]` would all fit: the choice is the one it gets with nothing else said,
    /// not whichever block came first. Returns whether anything was settled.
    pub(super) fn default_numbers(&mut self, ty: TypeId) -> bool {
        let ty = self.u.shallow(ty);
        match self.u.var_kind(ty) {
            Some(VarKind::Int) => {
                let int = self.u.table.i32;
                return self.u.unify(ty, int).is_ok();
            }
            Some(VarKind::Float) => {
                let float = self.u.table.f64;
                return self.u.unify(ty, float).is_ok();
            }
            Some(VarKind::Collection { element, len }) => {
                let settled = self.default_numbers(element);
                return self.default_numbers(len) || settled;
            }
            Some(_) => return false,
            None => {}
        }
        let children = self.u.get(ty).children();
        let mut settled = false;
        for child in children {
            settled |= self.default_numbers(child);
        }
        settled
    }
}
