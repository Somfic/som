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

impl TypeError {
    pub fn to_diagnostic_message(&self) -> String {
        match self {
            TypeError::Mismatch { expected, found, .. } => {
                format!("Type mismatch: expected {:?}, found {:?}", expected, found)
            }
            TypeError::InfiniteType { var, ty } => {
                format!("Infinite type: {:?} = {:?}", var, ty)
            }
            TypeError::MissingImpl { trait_id, self_type, arg_types } => {
                format!("Missing implementation for trait {:?} on {:?} with args {:?}", trait_id, self_type, arg_types)
            }
            TypeError::UnboundVariable { name } => {
                format!("Unbound variable: {}", name)
            }
            TypeError::WrongArgCount { expected, found } => {
                format!("Wrong argument count: expected {}, found {}", expected, found)
            }
            TypeError::Internal(msg) => {
                format!("Internal error: {}", msg)
            }
        }
    }
}
