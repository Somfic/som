//! Shared by the type checker, which has inference variables to look through, and lowering,
//! which works on concrete types.

use som_ast::{Func, Id};
use som_lang::LangTrait;
use som_resolve::DefId;

use crate::ty::{Ty, TypeId, TypeTable};

/// Indexed like `Names::impls`.
#[derive(Clone, Debug)]
pub struct ImplInfo {
    pub trait_: Option<DefId>,
    /// What `target` and `types` are written in terms of, as `Ty::Param`s.
    pub params: Vec<DefId>,
    /// `None` if it didn't resolve.
    pub target: Option<TypeId>,
    pub trait_args: Vec<TypeId>,
    /// For `target`, in the trait's order.
    pub types: Vec<Option<TypeId>>,
    /// As written, in the order written.
    pub written_types: Vec<TypeId>,
    pub methods: Vec<Id<Func>>,
    /// `impl<C: Index> IterRef for C`: the traits a type has to have for it to be the
    /// type's. `None` for one written for a type.
    pub blanket: Option<Vec<DefId>>,
}

impl ImplInfo {
    pub(crate) fn subst(&self, args: &[TypeId]) -> Vec<(DefId, TypeId)> {
        self.params
            .iter()
            .copied()
            .zip(args.iter().copied())
            .collect()
    }
}

/// `List<i32>` matches `List<T>`, binding `T` to `i32` in `bound`. `resolve` looks through
/// inference variables; one still unknown matches nothing but itself.
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

fn same(table: &TypeTable, a: TypeId, b: TypeId, resolve: &dyn Fn(TypeId) -> TypeId) -> bool {
    let (a, b) = (resolve(a), resolve(b));
    a == b
        || table
            .get(a)
            .zip_children(table.get(b))
            .is_some_and(|pairs| pairs.into_iter().all(|(x, y)| same(table, x, y, resolve)))
}

/// Its index, and each of its parameters for `ty` (the error type if `ty` doesn't say). With
/// `trait_args`, only an impl for those: `From<[i32; 3]>`, not any `From`.
///
/// An implementation written for the type comes first; a blanket one, `impl<C: Index>
/// IterRef for C`, only if there's none, and only for a type known to meet its bounds.
pub fn find_impl(
    impls: &[ImplInfo],
    table: &TypeTable,
    trait_: DefId,
    ty: TypeId,
    trait_args: &[TypeId],
    resolve: &dyn Fn(TypeId) -> TypeId,
) -> Option<(usize, Vec<TypeId>)> {
    find_impl_within(impls, table, trait_, ty, trait_args, resolve, 0)
}

/// How many blanket implementations' bounds can be followed, one inside another: two
/// written in terms of each other would go on forever.
const DEEPEST_BLANKET: usize = 8;

fn find_impl_within(
    impls: &[ImplInfo],
    table: &TypeTable,
    trait_: DefId,
    ty: TypeId,
    trait_args: &[TypeId],
    resolve: &dyn Fn(TypeId) -> TypeId,
    depth: usize,
) -> Option<(usize, Vec<TypeId>)> {
    let fitting = |i: usize, info: &ImplInfo| {
        let target = info.target.filter(|_| info.trait_ == Some(trait_))?;
        let mut bound = vec![None; info.params.len()];
        let fits = match_type(table, target, ty, &info.params, &mut bound, resolve)
            && (info.trait_args.iter().zip(trait_args)).all(|(&pattern, &arg)| {
                match_type(table, pattern, arg, &info.params, &mut bound, resolve)
            });
        fits.then(|| {
            let args = bound
                .into_iter()
                .map(|b| b.unwrap_or(table.error))
                .collect();
            (i, args)
        })
    };
    let own = (impls.iter().enumerate())
        .filter(|(_, info)| info.blanket.is_none())
        .find_map(|(i, info)| fitting(i, info));
    if own.is_some() {
        return own;
    }
    // A type still being worked out, or a type parameter, would fit any.
    let known = !matches!(
        table.get(resolve(ty)),
        Ty::Var(_) | Ty::Param(_) | Ty::Error
    );
    if !known || depth >= DEEPEST_BLANKET {
        return None;
    }
    (impls.iter().enumerate()).find_map(|(i, info)| {
        let bounds = info.blanket.as_ref()?;
        let found = fitting(i, info)?;
        let meets = (bounds.iter())
            .all(|&b| find_impl_within(impls, table, b, ty, &[], resolve, depth + 1).is_some());
        meets.then_some(found)
    })
}

pub fn substitute(table: &mut TypeTable, ty: TypeId, subst: &[(DefId, TypeId)]) -> TypeId {
    match table.get(ty).clone() {
        Ty::Param(def) => substituted(subst, def).unwrap_or(ty),
        other if other.children().is_empty() => ty,
        other => {
            let replaced = other.map_children(|child| substitute(table, child, subst));
            table.intern(replaced)
        }
    }
}

pub(crate) fn substituted(subst: &[(DefId, TypeId)], def: DefId) -> Option<TypeId> {
    subst.iter().find(|(d, _)| *d == def).map(|&(_, ty)| ty)
}

/// For concrete types, as lowering has them: `ListIntoIter<i32>::Item` is `i32`.
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
    let Some((i, args)) = find_impl(impls, table, trait_, base, &[], &|t| t) else {
        return inner;
    };
    let Some(written) = impls[i].types.get(index).copied().flatten() else {
        return inner;
    };
    let found = substitute(table, written, &impls[i].subst(&args));
    resolve_assoc(table, impls, found)
}

/// Whether values of type `ty` have the operators of trait `lang` built in, as numbers have
/// `+` and `<`: they meet a bound of it, and can't implement it themselves.
pub fn builtin_operator(ty: &Ty, lang: LangTrait) -> bool {
    match lang {
        LangTrait::Add | LangTrait::Subtract | LangTrait::Multiply | LangTrait::Divide => {
            matches!(ty, Ty::Int(_) | Ty::Float(_))
        }
        LangTrait::Compare => matches!(ty, Ty::Int(_) | Ty::Float(_) | Ty::Char),
        LangTrait::Eq => matches!(
            ty,
            Ty::Int(_) | Ty::Float(_) | Ty::Char | Ty::Bool | Ty::Pointer
        ),
        LangTrait::Iterator
        | LangTrait::IterRef
        | LangTrait::IterMut
        | LangTrait::IterOwned
        | LangTrait::From
        | LangTrait::Drop
        | LangTrait::Copy
        | LangTrait::Share
        | LangTrait::Index
        | LangTrait::IndexMut
        | LangTrait::Display => false,
    }
}

pub fn primitive_of(ty: &Ty) -> Option<som_ast::Primitive> {
    use crate::ty::{FloatTy, IntTy};
    use som_ast::Primitive as P;
    Some(match ty {
        Ty::Int(int) => match int {
            IntTy::I8 => P::I8,
            IntTy::I16 => P::I16,
            IntTy::I32 => P::I32,
            IntTy::I64 => P::I64,
            IntTy::I128 => P::I128,
            IntTy::ISize => P::ISize,
            IntTy::U8 => P::U8,
            IntTy::U16 => P::U16,
            IntTy::U32 => P::U32,
            IntTy::U64 => P::U64,
            IntTy::U128 => P::U128,
            IntTy::USize => P::USize,
        },
        Ty::Float(FloatTy::F32) => P::F32,
        Ty::Float(FloatTy::F64) => P::F64,
        Ty::Bool => P::Bool,
        Ty::Char => P::Char,
        Ty::Str => P::Str,
        Ty::CStr => P::CStr,
        _ => return None,
    })
}
