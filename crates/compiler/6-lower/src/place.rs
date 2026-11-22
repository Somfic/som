use som_ast::{Expr, Id};
use som_mir::*;
use som_resolve::{DefId, DefKind};

use super::FnBuilder;

impl FnBuilder<'_, '_> {
    pub(super) fn place_ty(&mut self, place: &Place) -> TypeId {
        let mut ty = self.locals[place.local].ty;
        for &projection in &place.projection {
            // A field of a generic struct's copy that may not have been made yet.
            if let Ty::Struct(def, args) = self.l.mir.types.get(ty).clone()
                && !args.is_empty()
                && !self.l.mir.has_struct(def, &args)
                && let DefKind::Struct(s) = self.l.names.def(def).kind
            {
                self.l.struct_instance(s, args);
            }
            ty = (self.l.mir.project(ty, projection)).expect("places are built well-typed");
        }
        ty
    }

    /// An `out` parameter is a pointer to the caller's variable, and its name means what it
    /// points to.
    pub(super) fn variable(&self, def: DefId) -> Option<Place> {
        if let Some(place) = self.captures.get(&def) {
            return Some(place.clone());
        }
        let place = Place::local(*self.bindings.get(&def)?);
        Some(match self.l.names.def(def).kind {
            DefKind::Param { func, index } if self.l.ast.funcs[func].params[index].out => {
                place.project(Projection::Deref)
            }
            _ => place,
        })
    }

    /// A narrowed optional's place is the value it holds.
    pub(super) fn place(&mut self, expr: Id<Expr>) -> Place {
        let mut place = self.place_of(expr);
        if self.typed.is_narrowed(expr) {
            place = place.project(Projection::Field(1));
        }
        // `n` in `n.len` of a `&&string`: read through to the `&string`.
        for _ in 0..self.typed.derefs(expr) {
            place = place.project(Projection::Deref);
        }
        place
    }

    /// Stored in a temporary if it isn't a place already.
    pub(super) fn place_of(&mut self, expr: Id<Expr>) -> Place {
        // A part of a variable the closure being built captures.
        if let Some(place) = self.capture_uses.get(&expr) {
            return place.clone();
        }
        let ast = self.l.ast;
        match &ast.exprs[expr] {
            Expr::Var(path) if path.int_limit().is_none() => {
                let def = (self.l.names.expr(expr)).expect("checked programs are fully resolved");
                if let Some(place) = self.variable(def) {
                    return place;
                }
            }
            Expr::Field { object, .. } if !self.is_prop(expr) => {
                return self.field_place(expr, *object);
            }
            // `values[i]` of a slice or struct: what its `at` lends, or `at_mut` where it's
            // changed. The prover checked `i` is below its `len`.
            Expr::Index { object, index } if let Some(def) = self.typed.method(expr) => {
                let DefKind::Func(func) = self.l.names.def(def).kind else {
                    unreachable!("methods are functions")
                };
                let span = self.span(expr);
                let call = self.call(expr, func, Some(*object), &[*index]);
                let Rvalue::Call { func: instance, .. } = &call else {
                    unreachable!("a call")
                };
                let lent_ty = self.l.mir.funcs[*instance].ret;
                let lent = self.temp(lent_ty, span.clone());
                self.assign(Place::local(lent), call, span);
                return Place::local(lent).project(Projection::Deref);
            }
            Expr::Index { object, index } if self.typed.array_method(expr).is_none() => {
                return self.index_place(*object, *index);
            }
            Expr::Deref(inner) => return self.place(*inner).project(Projection::Deref),
            _ => {}
        }
        let operand = self.operand(expr);
        self.into_place(operand, expr)
    }

    fn field_place(&mut self, expr: Id<Expr>, object: Id<Expr>) -> Place {
        let index = self.typed.field(expr).expect("checked fields are resolved");
        self.field_place_at(object, index)
    }

    /// Field `index` of `object`, through one reference or a `#T`.
    pub(super) fn field_place_at(&mut self, object: Id<Expr>, index: usize) -> Place {
        let mut object_ty = self.expr_ty(object);
        // `node.value` of a `#Node`: through the `&Node` its `get` lends.
        let mut place = if self.typed.is_shared_read(object) {
            let lent = self.shared_get(object);
            object_ty = self.body_local_ty(lent);
            Place::local(lent)
        } else {
            self.place(object)
        };
        // Fields are reachable through one reference.
        if let Ty::Ref { to, .. } = *self.l.mir.types.get(object_ty) {
            place = place.project(Projection::Deref);
            object_ty = to;
        }
        // An enum's common fields come right after its tag.
        let index = if self.is_enum(object_ty) {
            index + 1
        } else {
            index
        };
        place.project(Projection::Field(index))
    }

    /// `values[0]`, checked to be in bounds.
    fn index_place(&mut self, object: Id<Expr>, index: Id<Expr>) -> Place {
        let mut place = self.place(object);
        // Elements are reachable through one reference.
        let object_ty = self.expr_ty(object);
        if let Ty::Ref { .. } = *self.l.mir.types.get(object_ty) {
            place = place.project(Projection::Deref);
        }
        let span = self.span(index);
        let position = self.temp(self.l.mir.types.usize, span.clone());
        let at = self.operand(index);
        let index_ty = self.expr_ty(index);
        let at = self.usize_rvalue(at, index_ty);
        self.assign(Place::local(position), at, span);
        place.project(Projection::Index(position))
    }
}
