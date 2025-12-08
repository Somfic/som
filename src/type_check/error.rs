use crate::{type_check::Provenance, TraitId, Type, TypeId, TypeVar};

#[derive(Debug)]
pub enum TypeError {
    Mismatch {
        expected: Type,
        found: Type,
        provenance: Provenance,
        expected_type_id: Option<TypeId>, // For showing where the expected type was declared
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
    WrongArgCount {
        expected: usize,
        found: usize,
    },
    Internal(String),
}
