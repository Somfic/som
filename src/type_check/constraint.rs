use crate::{ExprId, TraitId, Type, TypeId};

#[derive(Debug, Clone)]
pub enum Constraint {
    Equal {
        provenance: Provenance,
        lhs: Type,
        rhs: Type,
    },
    Trait {
        provenance: Provenance,
        trait_id: TraitId,
        args: Vec<Type>,
        output: Type,
    },
}

impl Constraint {
    pub fn expr_id(&self) -> ExprId {
        match self {
            Constraint::Equal { provenance, .. } => provenance.expr_id(),
            Constraint::Trait { provenance, .. } => provenance.expr_id(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Provenance {
    BinaryOp(ExprId),
    FunctionCall(ExprId, Option<TypeId>), // ExprId of the return value, TypeId of the expected return type annotation
    LetBinding(ExprId),
    Annotation(ExprId),
    Check(ExprId),
    FunctionArity,
    Unification,
    Deref(ExprId),
}

impl Provenance {
    pub fn expr_id(&self) -> ExprId {
        match self {
            Provenance::BinaryOp(id) => *id,
            Provenance::FunctionCall(id, _) => *id,
            Provenance::LetBinding(id) => *id,
            Provenance::Annotation(id) => *id,
            Provenance::Check(id) => *id,
            Provenance::FunctionArity | Provenance::Unification => {
                panic!("Provenance {:?} has no expr_id", self)
            }
            Provenance::Deref(id) => *id,
        }
    }

    pub fn expected_type_id(&self) -> Option<TypeId> {
        match self {
            Provenance::FunctionCall(_, type_id) => *type_id,
            _ => None,
        }
    }
}
