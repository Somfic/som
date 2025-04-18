use super::Identifier;
use miette::SourceSpan;
use span_derive::Span;
use std::fmt::Display;

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

#[derive(Debug, Clone, Eq)]
pub enum TypingValue<'ast> {
    Unknown,
    Integer,
    Boolean,
    Decimal,
    Unit,
    Symbol(Identifier<'ast>),
    Generic(Identifier<'ast>),
    Struct(Vec<StructMember<'ast>>),
}

impl PartialEq for TypingValue<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Symbol(l0), Self::Symbol(r0)) => l0 == r0,
            (Self::Generic(l0), Self::Generic(r0)) => l0 == r0,
            (Self::Struct(lfields), Self::Struct(rfields)) => {
                if lfields.len() != rfields.len() {
                    return false;
                }
                lfields
                    .iter()
                    .all(|m| rfields.iter().any(|n| m.name == n.name && m.ty == n.ty))
            }
            (Self::Integer, Self::Integer) => true,
            (Self::Decimal, Self::Decimal) => true,
            (Self::Boolean, Self::Boolean) => true,
            (Self::Unit, Self::Unit) => true,
            (Self::Unknown, Self::Unknown) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructMember<'ast> {
    pub name: Identifier<'ast>,
    pub ty: Typing<'ast>,
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
            TypingValue::Symbol(name) => write!(f, "{}", name),
            TypingValue::Generic(identifier) => write!(f, "`{}", identifier),
            TypingValue::Unit => write!(f, "nothing"),
            TypingValue::Struct(members) => write!(
                f,
                "{{ {} }}",
                members
                    .iter()
                    .map(|m| format!("{} ~ {}", m.name, m.ty))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}
