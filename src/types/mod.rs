use std::fmt::Display;
use std::hash::Hash;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

impl Hash for Type {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
}

impl Type {
    pub fn new(source: impl Into<Span>, kind: TypeKind) -> Self {
        Self {
            kind,
            span: source.into(),
        }
    }

    pub fn equals(&self, other: &Type) -> bool {
        self.kind == other.kind
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl Display for TypeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeKind::Never => write!(f, "never"),
            TypeKind::Integer => write!(f, "an integer"),
            TypeKind::Boolean => write!(f, "a boolean"),
        }
    }
}

impl From<Type> for Span {
    fn from(ty: Type) -> Self {
        ty.span
    }
}

impl From<&Type> for Span {
    fn from(ty: &Type) -> Self {
        ty.span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeKind {
    Never,
    Integer,
    Boolean,
}
