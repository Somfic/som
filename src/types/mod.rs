use crate::prelude::*;

pub struct Type {
    pub kind: TypeKind,
    pub span: SourceSpan,
}

impl Type {
    pub fn new(source: impl Into<SourceSpan>, kind: TypeKind) -> Self {
        Self {
            kind,
            span: source.into(),
        }
    }

    pub fn equals(&self, other: &Type) -> bool {
        self.kind == other.kind
    }
}

impl From<Type> for SourceSpan {
    fn from(ty: Type) -> Self {
        ty.span
    }
}

impl From<&Type> for SourceSpan {
    fn from(ty: &Type) -> Self {
        ty.span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Integer,
    Boolean,
}
