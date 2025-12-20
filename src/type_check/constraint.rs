use crate::{Expr, Trait, Type, arena::Id};

#[derive(Debug, Clone)]
pub enum Constraint {
    Equal {
        provenance: Provenance,
        lhs: Type,
        rhs: Type,
    },
    Trait {
        provenance: Provenance,
        trait_id: Id<Trait>,
        args: Vec<Type>,
        output: Type,
    },
}

impl Constraint {
    pub fn expr_id(&self) -> Id<Expr> {
        match self {
            Constraint::Equal { provenance, .. } => provenance.expr_id(),
            Constraint::Trait { provenance, .. } => provenance.expr_id(),
        }
    }

    pub fn provenance(&self) -> &Provenance {
        match self {
            Constraint::Equal { provenance, .. } => provenance,
            Constraint::Trait { provenance, .. } => provenance,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Provenance {
    BinaryOp(Id<Expr>),
    FunctionCall(Id<Expr>, Option<Id<Type>>), // ExprId of the return value, TypeId of the expected return type annotation
    FuncArg {
        arg_expr: Id<Expr>,
        param_type_id: Option<Id<Type>>,
    },
    LetBinding(Id<Expr>),
    Annotation(Id<Expr>),
    Check(Id<Expr>),
    FunctionArity,
    Unification,
    Deref(Id<Expr>),
}

impl Provenance {
    pub fn expr_id(&self) -> Id<Expr> {
        match self {
            Provenance::BinaryOp(id) => *id,
            Provenance::FunctionCall(id, _) => *id,
            Provenance::FuncArg { arg_expr, .. } => *arg_expr,
            Provenance::LetBinding(id) => *id,
            Provenance::Annotation(id) => *id,
            Provenance::Check(id) => *id,
            Provenance::FunctionArity | Provenance::Unification => {
                panic!("Provenance {:?} has no expr_id", self)
            }
            Provenance::Deref(id) => *id,
        }
    }

    pub fn expected_type_id(&self) -> Option<Id<Type>> {
        match self {
            Provenance::FunctionCall(_, type_id) => *type_id,
            Provenance::FuncArg { param_type_id, .. } => *param_type_id,
            _ => None,
        }
    }
}
