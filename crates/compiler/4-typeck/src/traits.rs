//! Traits: which implementation a type has, and what its associated types are. Shared by the
//! type checker, which has inference variables to look through, and lowering, which works on
//! concrete types.

use som_ast::{Func, Id};
use som_resolve::DefId;

use crate::ty::{Ty, TypeId, TypeTable};

/// An `impl` block, as the type checker knows it. Indexed like `Names::impls`.
#[derive(Clone, Debug)]
pub struct ImplInfo {
    /// The trait it implements, if it does.
    pub trait_: Option<DefId>,
    /// Its parameters, which `target` and `types` are written in terms of (`Ty::Param` of
    /// each).
    pub params: Vec<DefId>,
    /// What it's for: `ListIter<T>`. `None` if that didn't resolve.
    pub target: Option<TypeId>,
    /// What the trait's associated types are for `target`, in the trait's order.
    pub types: Vec<Option<TypeId>>,
    /// The associated types as written, in the order written.
    pub written_types: Vec<TypeId>,
    pub methods: Vec<Id<Func>>,
}

/// Whether `ty` is what `pattern`, written in terms of `params`, describes: `List<i32>` is
/// `List<T>`, with `T` bound to `i32` in `bound`. `resolve` looks through inference
/// variables; one that's still unknown matches nothing but itself.
pub fn match_type(
    table: &TypeTable,
    pattern: TypeId,
    ty: TypeId,
    params: &[DefId],
    bound: &mut [Option<TypeId>],
    resolve: &dyn Fn(TypeId) -> TypeId,
) -> bool {
    let ty = resolve(ty);
    match table.get(pattern) {
        Ty::Param(def) if let Some(i) = params.iter().position(|p| p == def) => match bound[i] {
            Some(earlier) => same(table, earlier, ty, resolve),
            None => {
                bound[i] = Some(ty);
                true
            }
        },
        Ty::Error => true,
        p => match p.zip_children(table.get(ty)) {
            Some(pairs) => {
                (pairs.into_iter()).all(|(p, t)| match_type(table, p, t, params, bound, resolve))
            }
            None => false,
        },
    }
}

/// Whether `a` and `b` are the same type, looking through inference variables.
fn same(table: &TypeTable, a: TypeId, b: TypeId, resolve: &dyn Fn(TypeId) -> TypeId) -> bool {
    let (a, b) = (resolve(a), resolve(b));
    a == b
        || table
            .get(a)
            .zip_children(table.get(b))
            .is_some_and(|pairs| pairs.into_iter().all(|(x, y)| same(table, x, y, resolve)))
}

/// The implementation of trait `trait_` for `ty`, among `impls`: its index, and what each of
/// its parameters is for `ty` (the error type if `ty` doesn't say).
pub fn find_impl(
    impls: &[ImplInfo],
    table: &TypeTable,
    trait_: DefId,
    ty: TypeId,
    resolve: &dyn Fn(TypeId) -> TypeId,
) -> Option<(usize, Vec<TypeId>)> {
    impls.iter().enumerate().find_map(|(i, info)| {
        let target = info.target.filter(|_| info.trait_ == Some(trait_))?;
        let mut bound = vec![None; info.params.len()];
        match_type(table, target, ty, &info.params, &mut bound, resolve).then(|| {
            let args = bound
                .into_iter()
                .map(|b| b.unwrap_or(table.error))
                .collect();
            (i, args)
        })
    })
}

/// `ty` with `params` replaced by `args`.
pub fn substitute(table: &mut TypeTable, ty: TypeId, params: &[DefId], args: &[TypeId]) -> TypeId {
    match table.get(ty).clone() {
        Ty::Param(def) => match params.iter().position(|&p| p == def) {
            Some(i) => args[i],
            None => ty,
        },
        other if other.children().is_empty() => ty,
        other => {
            let replaced = other.map_children(|child| substitute(table, child, params, args));
            table.intern(replaced)
        }
    }
}

/// `ty` with every associated type of a known type worked out through its implementation:
/// `ListIter<i32>::Item` is `&i32`. For concrete types, as lowering has them.
pub fn resolve_assoc(table: &mut TypeTable, impls: &[ImplInfo], ty: TypeId) -> TypeId {
    let t = table.get(ty).clone();
    if t.children().is_empty() {
        return ty;
    }
    let inner = t.map_children(|child| resolve_assoc(table, impls, child));
    let inner = table.intern(inner);
    let Ty::Assoc {
        base,
        trait_,
        index,
    } = *table.get(inner)
    else {
        return inner;
    };
    let Some((i, args)) = find_impl(impls, table, trait_, base, &|t| t) else {
        return inner;
    };
    let Some(written) = impls[i].types.get(index).copied().flatten() else {
        return inner;
    };
    let params = impls[i].params.clone();
    let found = substitute(table, written, &params, &args);
    resolve_assoc(table, impls, found)
}
