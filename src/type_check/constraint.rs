use crate::{ExprId, TraitId, Type};

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
    FunctionCall(ExprId),
    LetBinding(ExprId),
    Annotation(ExprId),
    Check(ExprId),
    FunctionArity,
    Unification,
}

impl Provenance {
    pub fn expr_id(&self) -> ExprId {
        match self {
            Provenance::BinaryOp(id) => *id,
            Provenance::FunctionCall(id) => *id,
            Provenance::LetBinding(id) => *id,
            Provenance::Annotation(id) => *id,
            Provenance::Check(id) => *id,
            Provenance::FunctionArity | Provenance::Unification => {
                panic!("Provenance {:?} has no expr_id", self)
            }
        }
    }
}
