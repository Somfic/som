use som_ast::{Func, Id, Struct};
use som_driver::Analysis;
use som_resolve::DefId;
use som_typeck::{Ty, TypeId};

/// What type parameters stand for, as `display_with` takes them.
pub(crate) type Subst = Vec<(DefId, TypeId)>;

pub(crate) fn unref(a: &Analysis, t: TypeId) -> TypeId {
    match a.typed.ty(t) {
        Ty::Ref { to, .. } => *to,
        _ => t,
    }
}

pub(crate) fn struct_of(a: &Analysis, t: TypeId) -> Option<DefId> {
    match a.typed.ty(unref(a, t)) {
        Ty::Struct(def, _) => Some(*def),
        _ => None,
    }
}

/// What struct `s`'s type parameters stand for in `t`: `T` is `i32` in a `Vector<i32>`.
pub(crate) fn struct_subst(a: &Analysis, s: Id<Struct>, t: TypeId) -> Subst {
    match a.typed.ty(unref(a, t)) {
        Ty::Struct(_, args) => (a.names.struct_params(s).iter().copied())
            .zip(args.iter().copied())
            .collect(),
        _ => Vec::new(),
    }
}

/// What method `f`'s type parameters are when it's called on `receiver`, worked out from
/// its `self` parameter.
pub(crate) fn method_subst(a: &Analysis, f: Id<Func>, receiver: TypeId) -> Subst {
    let Some(&self_ty) = a.typed.func(f).params.first() else {
        return Vec::new();
    };
    match (
        a.typed.ty(unref(a, self_ty)),
        a.typed.ty(unref(a, receiver)),
    ) {
        (Ty::Struct(_, written), Ty::Struct(_, actual)) => written
            .iter()
            .zip(actual)
            .filter_map(|(&w, &actual)| match a.typed.ty(w) {
                Ty::Param(def) => Some((*def, actual)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether a value of type `ty` could be what an impl block for `target` is for:
/// `Vector<3, i32>` is `Vector<3, T>`'s, not `Vector<2, T>`'s.
pub(crate) fn impl_target_matches(a: &Analysis, target: TypeId, ty: TypeId) -> bool {
    let (target, ty) = (unref(a, target), unref(a, ty));
    match (a.typed.ty(target), a.typed.ty(ty)) {
        (Ty::Param(_), _) | (_, Ty::Error) => true,
        (Ty::Struct(d, xs), Ty::Struct(e, ys)) => {
            d == e
                && xs.len() == ys.len()
                && (xs.iter().zip(ys)).all(|(&x, &y)| impl_target_matches(a, x, y))
        }
        _ => target == ty,
    }
}

/// Whether `ty`, in method `func`'s terms, is the struct its impl block is for, with the
/// method's own copies of the block's parameters. A built-in type's isn't: `i32` reads
/// better than `Self`.
pub(crate) fn is_self(a: &Analysis, func: Id<Func>, ty: TypeId) -> bool {
    let Some(index) = a.names.impl_of(func) else {
        return false;
    };
    let info = &a.typed.impls[index];
    let Some(target) = info.target else {
        return false;
    };
    if !matches!(a.typed.ty(target), Ty::Struct(..)) {
        return false;
    }
    let own = a.names.type_params(func);
    let own = &own[..info.params.len().min(own.len())];
    same_shape(a, &info.params, own, target, ty)
}

fn same_shape(a: &Analysis, params: &[DefId], own: &[DefId], target: TypeId, ty: TypeId) -> bool {
    match (a.typed.ty(target), a.typed.ty(ty)) {
        (Ty::Param(p), Ty::Param(q)) => {
            let at = |list: &[DefId], d| list.iter().position(|x| x == d);
            at(params, p).is_some_and(|i| at(own, q) == Some(i))
        }
        (Ty::Struct(d, xs), Ty::Struct(e, ys)) => {
            d == e
                && xs.len() == ys.len()
                && (xs.iter().zip(ys)).all(|(&x, &y)| same_shape(a, params, own, x, y))
        }
        _ => target == ty,
    }
}
