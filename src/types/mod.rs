use std::fmt::Display;
use std::hash::Hash;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    pub value: TypeValue,
    pub span: Span,
}

impl Hash for Type {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl Type {
    pub fn new(source: impl Into<Span>, value: TypeValue) -> Self {
        Self {
            value,
            span: source.into(),
        }
    }

    pub fn equals(&self, other: &Type) -> bool {
        self.value == other.value
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Display for TypeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeValue::Never => write!(f, "never"),
            TypeValue::Integer => write!(f, "an integer"),
            TypeValue::Boolean => write!(f, "a boolean"),
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
pub enum TypeValue {
    Never,
    Integer,
    Boolean,
}
