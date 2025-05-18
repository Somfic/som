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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Integer,
}
