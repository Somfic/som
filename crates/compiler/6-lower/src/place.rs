use som_ast::{Expr, Id};
use som_mir::*;
use som_resolve::{DefId, DefKind};

use super::FnBuilder;

impl FnBuilder<'_, '_> {
    /// The type of `place` in the body being built.
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

    /// Where a parameter or local lives. An `out` parameter is a pointer to the caller's
    /// variable, and its name means what it points to.
    pub(super) fn variable(&self, def: DefId) -> Option<Place> {
        let place = Place::local(*self.bindings.get(&def)?);
        Some(match self.l.names.def(def).kind {
            DefKind::Param { func, index } if self.l.ast.funcs[func].params[index].out => {
                place.project(Projection::Deref)
            }
            _ => place,
        })
    }

    pub(super) fn place(&mut self, expr: Id<Expr>) -> Place {
        let place = self.place_of(expr);
        if self.typed.is_narrowed(expr) {
            place.project(Projection::Field(1))
        } else {
            place
        }
    }

    pub(super) fn place_of(&mut self, expr: Id<Expr>) -> Place {
        let ast = self.l.ast;
        match &ast.exprs[expr] {
            Expr::Var(path) if path.int_limit().is_none() => {
                let def = self
                    .l
                    .names
                    .expr(expr)
                    .expect("checked programs are fully resolved");
                if let Some(place) = self.variable(def) {
                    return place;
                }
            }
            Expr::Field { object, .. } if !self.is_prop(expr) => {
                let index = self.typed.field(expr).expect("checked fields are resolved");
                let mut object_ty = self.expr_ty(*object);
                // `node.value` of a `#Node`: through the `&Node` its `get` lends.
                let mut place = if self.typed.is_shared_read(*object) {
                    let lent = self.shared_get(*object);
                    object_ty = self.body_local_ty(lent);
                    Place::local(lent)
                } else {
                    self.place(*object)
                };
                // Fields are reachable through one reference.
                if let Ty::Ref { to, .. } = *self.l.mir.types.get(object_ty) {
                    place = place.project(Projection::Deref);
                    object_ty = to;
                }
                // An enum's common fields come right after its tag.
                let is_enum = matches!(self.l.mir.types.get(object_ty), Ty::Struct(def, _)
                    if matches!(self.l.names.def(*def).kind,
                        DefKind::Struct(s) if self.l.ast.structs[s].is_enum()));
                let index = if is_enum { index + 1 } else { index };
                return place.project(Projection::Field(index));
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
            // `values[0]`: checked to be in bounds.
            Expr::Index { object, index } if self.typed.array_method(expr).is_none() => {
                let mut place = self.place(*object);
                // Elements are reachable through one reference.
                let object_ty = self.expr_ty(*object);
                if let Ty::Ref { .. } = *self.l.mir.types.get(object_ty) {
                    place = place.project(Projection::Deref);
                }
                let usize = self.l.mir.types.intern(Ty::Int(IntTy::USize));
                let span = self.span(*index);
                let position = self.temp(usize, span.clone());
                let at = self.operand(*index);
                // Known to be inside: any integer type, as a position.
                let at = if self.expr_ty(*index) == usize {
                    Rvalue::Use(at)
                } else {
                    Rvalue::Cast {
                        operand: at,
                        ty: usize,
                    }
                };
                self.assign(Place::local(position), at, span);
                return place.project(Projection::Index(position));
            }
            Expr::Deref(inner) => return self.place(*inner).project(Projection::Deref),
            _ => {}
        }

        match self.operand(expr) {
            Operand::Copy(place) | Operand::Move(place) => place,
            constant => {
                let ty = self.expr_ty(expr);
                let span = self.span(expr);
                let temp = self.temp(ty, span.clone());
                self.assign(Place::local(temp), Rvalue::Use(constant), span);
                Place::local(temp)
            }
        }
    }
}
