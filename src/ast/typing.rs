use miette::SourceSpan;
use span_derive::Span;
use std::{borrow::Cow, fmt::Display};

use super::Identifier;

#[derive(Debug, Clone, Span)]
pub struct Typing<'ast> {
    pub value: TypingValue<'ast>,
    pub span: SourceSpan,
}

impl PartialEq for Typing<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Eq for Typing<'_> {}

impl<'ast> Typing<'ast> {
    pub fn unknown(span: &SourceSpan) -> Self {
        Self {
            value: TypingValue::Unknown,
            span: *span,
        }
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self.value, TypingValue::Unknown)
    }

    pub fn integer(span: &SourceSpan) -> Self {
        Self {
            value: TypingValue::Integer,
            span: *span,
        }
    }

    pub fn decimal(span: &SourceSpan) -> Self {
        Self {
            value: TypingValue::Decimal,
            span: *span,
        }
    }

    pub fn symbol(span: &SourceSpan, name: Identifier<'ast>) -> Self {
        Self {
            value: TypingValue::Symbol(name),
            span: *span,
        }
    }

    pub fn boolean(span: &SourceSpan) -> Self {
        Self {
            value: TypingValue::Boolean,
            span: *span,
        }
    }

    pub fn unit(span: &SourceSpan) -> Self {
        Self {
            value: TypingValue::Unit,
            span: *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypingValue<'ast> {
    Unknown,
    Integer,
    Boolean,
    Decimal,
    Unit,
    Generic(Identifier<'ast>),
    Symbol(Identifier<'ast>),
    Struct(Identifier<'ast>),
}

impl<'ast> TypingValue<'ast> {
    pub fn with_span(self, span: miette::SourceSpan) -> Typing<'ast> {
        Typing { value: self, span }
    }
}

impl Display for Typing<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Display for TypingValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            TypingValue::Unknown => write!(f, "unknown"),
            TypingValue::Integer => write!(f, "an integer"),
            TypingValue::Decimal => write!(f, "a decimal"),
            TypingValue::Boolean => write!(f, "a boolean"),
            TypingValue::Symbol(identifier) => write!(f, "{}", identifier),
            TypingValue::Generic(identifier) => write!(f, "`{}", identifier),
            TypingValue::Unit => write!(f, "nothing"),
            TypingValue::Struct(identifier) => write!(f, "struct {}", identifier),
        }
    }
}
