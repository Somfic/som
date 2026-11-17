use crate::FnChecker;
use som_mir::*;
use std::fmt;

/// A place as it would be written in som: `p.x`, `*r`.
pub(crate) struct PlaceName {
    /// `None` for a temporary.
    local: Option<String>,
    derefs: usize,
    /// The fields and indexes after the local: `.x[_]`.
    path: String,
}

impl PlaceName {
    /// `None` for a temporary reached without naming any part of it.
    pub(crate) fn if_named(self) -> Option<String> {
        (self.local.is_some() || self.derefs > 0 || !self.path.is_empty()).then(|| self.to_string())
    }
}

impl fmt::Display for PlaceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let local = self.local.as_deref().unwrap_or("temporary value");
        write!(f, "{}{local}{}", "*".repeat(self.derefs), self.path)
    }
}

impl FnChecker<'_> {
    pub(crate) fn name(&self, place: &Place) -> String {
        self.place_name(place).to_string()
    }

    pub(crate) fn place_name(&self, place: &Place) -> PlaceName {
        let decl = self.body.local(place.local);
        let mut name = PlaceName {
            local: decl.name.clone(),
            derefs: 0,
            path: String::new(),
        };
        let mut ty = decl.ty;
        let mut projections = place.projection.iter().peekable();
        while let Some(projection) = projections.next() {
            match (projection, self.mir.types.get(ty)) {
                (Projection::Deref, Ty::Ref { to, .. }) => {
                    // A struct's fields are reached through a reference without the `*`:
                    // `u.name`, not `*u.name`.
                    let field_of_struct = matches!(projections.peek(), Some(Projection::Field(_)))
                        && matches!(self.mir.types.get(*to), Ty::Struct(..));
                    if !field_of_struct {
                        name.derefs += 1;
                    }
                    ty = *to;
                }
                // A variant's own fields are named by the enum: `match m` moved from `m`.
                (Projection::Field(i), Ty::Struct(def, args))
                    if self.mir.struct_def(*def, args).variants.is_some()
                        && self.mir.struct_def(*def, args).fields[*i].0.contains('.') =>
                {
                    break;
                }
                (Projection::Field(i), Ty::Struct(def, args)) => {
                    let (field, field_ty) = &self.mir.struct_def(*def, args).fields[*i];
                    name.path += &format!(".{field}");
                    ty = *field_ty;
                }
                // Only the value an optional holds can be borrowed or moved: it's what
                // `if let` binds, so it goes by the optional's own name.
                (Projection::Field(i), Ty::Optional(inner)) => {
                    ty = if *i == 0 { self.mir.types.bool } else { *inner };
                }
                (Projection::Index(_), Ty::Array { element, .. } | Ty::Slice(element)) => {
                    name.path += "[_]";
                    ty = *element;
                }
                (Projection::Index(_), Ty::Str) => {
                    name.path += "[_]";
                    ty = self.mir.types.u8;
                }
                // Where a slice's elements start, or how many there are: part of the slice.
                (Projection::Field(_), Ty::Ref { .. }) => {
                    ty = self.mir.project(ty, *projection).expect("validated MIR");
                }
                _ => unreachable!("validated MIR has well-typed places"),
            }
        }
        name
    }
}
