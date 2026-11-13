//! Whether a `match` covers every value, and which arms can never be reached.
//!
//! The usual "usefulness" algorithm: a pattern is useful after some others if there's a value
//! it matches and none of them do. A `match` covers everything if a wildcard after its arms
//! isn't useful, and an arm is reachable if it's useful after the arms before it. When a
//! wildcard is useful, the value it alone matches is a witness: what the error names as not
//! matched.

use crate::ty::TypeId;

/// One way of making a value: a variant, `true`, `5`, `none`, or an optional holding a value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Ctor {
    Variant(usize),
    Bool(bool),
    Int(i64),
    None,
    Some,
}

/// A pattern reduced to what decides which values it matches.
#[derive(Clone, Debug)]
pub(crate) enum Pat {
    /// Matches anything: `_`, or a name that binds.
    Wild,
    /// Values made with the constructor, whose fields match the patterns.
    Ctor(Ctor, Vec<Pat>),
}

/// The ways of making values of a type, each with the types of what it holds; `None` if
/// they're too many to list, like an integer's.
pub(crate) type Space<'a> = dyn FnMut(TypeId) -> Option<Vec<(Ctor, Vec<TypeId>)>> + 'a;

/// A value `pats` matches that no row of `rows` does, if there is one: patterns for it, one
/// for each of `tys`. Every row, like `pats`, has a pattern for each of `tys`.
pub(crate) fn useful(
    rows: &[Vec<Pat>],
    pats: &[Pat],
    tys: &[TypeId],
    space: &mut Space,
) -> Option<Vec<Pat>> {
    let Some((head, rest)) = pats.split_first() else {
        // Nothing left to tell values apart: only useful if no row matched already.
        return rows.is_empty().then(Vec::new);
    };
    let ty = tys[0];
    match head {
        Pat::Ctor(ctor, fields) => {
            let field_tys = field_types(space, ty, ctor, fields.len());
            specialized(
                rows,
                rest,
                &tys[1..],
                ctor,
                fields.clone(),
                field_tys,
                space,
            )
        }
        Pat::Wild => {
            let all = space(ty);
            let used: Vec<&Ctor> = (rows.iter())
                .filter_map(|row| match &row[0] {
                    Pat::Ctor(ctor, _) => Some(ctor),
                    Pat::Wild => None,
                })
                .collect();
            // Every way of making the value has a row: try each.
            if let Some(all) = &all
                && all.iter().all(|(ctor, _)| used.contains(&ctor))
            {
                return all.iter().find_map(|(ctor, field_tys)| {
                    let wilds = vec![Pat::Wild; field_tys.len()];
                    specialized(rows, rest, &tys[1..], ctor, wilds, field_tys.clone(), space)
                });
            }
            // Some way of making it has none: only the rows matching anything decide the rest.
            let defaults: Vec<Vec<Pat>> = (rows.iter())
                .filter(|row| matches!(row[0], Pat::Wild))
                .map(|row| row[1..].to_vec())
                .collect();
            let witness = useful(&defaults, rest, &tys[1..], space)?;
            let missing = match all {
                Some(all) => (all.into_iter())
                    .find(|(ctor, _)| !used.contains(&ctor))
                    .map(|(ctor, field_tys)| Pat::Ctor(ctor, vec![Pat::Wild; field_tys.len()]))
                    .unwrap_or(Pat::Wild),
                None => Pat::Wild,
            };
            Some(std::iter::once(missing).chain(witness).collect())
        }
    }
}

/// `useful` for values made with `ctor`, whose fields `fields` match, of types `field_tys`.
fn specialized(
    rows: &[Vec<Pat>],
    rest: &[Pat],
    rest_tys: &[TypeId],
    ctor: &Ctor,
    fields: Vec<Pat>,
    field_tys: Vec<TypeId>,
    space: &mut Space,
) -> Option<Vec<Pat>> {
    let arity = fields.len();
    let rows: Vec<Vec<Pat>> = rows
        .iter()
        .filter_map(|row| specialize(row, ctor, arity))
        .collect();
    let pats: Vec<Pat> = fields.into_iter().chain(rest.iter().cloned()).collect();
    let tys: Vec<TypeId> = field_tys
        .into_iter()
        .chain(rest_tys.iter().copied())
        .collect();
    let mut witness = useful(&rows, &pats, &tys, space)?;
    let rest = witness.split_off(arity);
    Some(
        std::iter::once(Pat::Ctor(ctor.clone(), witness))
            .chain(rest)
            .collect(),
    )
}

/// `row` for values made with `ctor`, which holds `arity` fields: its patterns for those
/// fields, then the rest. `None` if it can't match such a value.
fn specialize(row: &[Pat], ctor: &Ctor, arity: usize) -> Option<Vec<Pat>> {
    let (head, rest) = row.split_first().expect("rows are as long as the patterns");
    let fields = match head {
        Pat::Ctor(other, fields) if other == ctor => fields.clone(),
        Pat::Ctor(..) => return None,
        Pat::Wild => vec![Pat::Wild; arity],
    };
    Some(fields.into_iter().chain(rest.iter().cloned()).collect())
}

/// The types of what `ctor` holds, for a value of type `ty`.
fn field_types(space: &mut Space, ty: TypeId, ctor: &Ctor, arity: usize) -> Vec<TypeId> {
    (space(ty).into_iter().flatten())
        .find(|(c, _)| c == ctor)
        .map(|(_, tys)| tys)
        .filter(|tys| tys.len() == arity)
        .unwrap_or_else(|| vec![ty; arity])
}
