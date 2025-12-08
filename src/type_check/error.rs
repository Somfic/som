use crate::{type_check::Provenance, TraitId, Type, TypeVar};

#[derive(Debug)]
pub enum TypeError {
    Mismatch {
        expected: Type,
        found: Type,
        provenance: Provenance,
    },
    InfiniteType {
        var: TypeVar,
        ty: Type,
    },
    MissingImpl {
        trait_id: TraitId,
        self_type: Type,
        arg_types: Vec<Type>,
    },
    UnboundVariable {
        name: String,
    },
    Internal(String),
}
