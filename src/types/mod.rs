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
            TypeValue::Unit => write!(f, "nothing"),
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
    /// This type is only ever used internally by the type checker to indicate that a value is undetermined or invalid.
    Never,
    /// This type is only ever used internally by the type checker to indicate that a value does not have a type. For example the type of an expression block with only statements and no last expression.
    Unit,
    Integer,
    Boolean,
}
