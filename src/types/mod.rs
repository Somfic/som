use std::fmt::{write, Display};
use std::hash::Hash;

use crate::expressions::function::Parameter;
use crate::prelude::*;

pub mod integer;

#[derive(Debug, Clone, Eq)]
pub struct Type {
    pub value: TypeValue,
    pub span: Span,
}

impl From<Type> for miette::SourceSpan {
    fn from(ty: Type) -> Self {
        ty.span.into()
    }
}

impl From<&Type> for miette::SourceSpan {
    fn from(ty: &Type) -> Self {
        ty.span.into()
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
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

    pub fn with_span(self, span: impl Into<Span>) -> Self {
        Self {
            value: self.value.clone(),
            span: span.into(),
        }
    }

    pub fn with_value(self, value: TypeValue) -> Self {
        Self {
            value,
            span: self.span,
        }
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
            TypeValue::Integer => write!(f, "int"),
            TypeValue::Boolean => write!(f, "bool"),
            TypeValue::Unit => write!(f, "nothing"),
            TypeValue::Function(function) => {
                let params = function
                    .parameters
                    .iter()
                    .map(|p| format!("{}", p.type_))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "fn({}) -> {}", params, function.returns)
            }
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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TypeValue {
    /// This type is only ever used internally by the type checker to indicate that a value is undetermined or invalid.
    Never,
    /// This type is only ever used internally by the type checker to indicate that a value does not have a type. For example the type of an expression block with only statements and no last expression.
    Unit,
    Integer,
    Boolean,
    Function(FunctionType),
}

impl TypeValue {
    pub fn to_ir(&self) -> CompilerType::Type {
        match self {
            TypeValue::Integer => CompilerType::I64,
            TypeValue::Boolean => CompilerType::I8,
            TypeValue::Unit => CompilerType::I8,
            TypeValue::Function(function) => todo!(),
            TypeValue::Never => CompilerType::I8,
        }
    }
}

#[derive(Debug, Clone, Eq)]
pub struct FunctionType {
    pub parameters: Vec<Parameter>,
    pub returns: Box<Type>,
    pub span: Span,
}

impl From<FunctionType> for miette::SourceSpan {
    fn from(function: FunctionType) -> Self {
        function.span.into()
    }
}

impl From<&FunctionType> for miette::SourceSpan {
    fn from(function: &FunctionType) -> Self {
        function.span.into()
    }
}

impl PartialEq for FunctionType {
    fn eq(&self, other: &Self) -> bool {
        self.parameters == other.parameters && self.returns == other.returns
    }
}

impl Hash for FunctionType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.parameters.hash(state);
        self.returns.hash(state);
    }
}
