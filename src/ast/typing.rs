use miette::SourceSpan;
use std::{borrow::Cow, fmt::Display};

#[derive(Debug, Clone)]
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

    pub fn symbol(span: &SourceSpan, name: Cow<'ast, str>) -> Self {
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
}

impl Typing<'_> {
    pub fn label(&self, text: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, text.into())
    }

    pub fn span(mut self, span: SourceSpan) -> Self {
        self.span = span;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypingValue<'ast> {
    Unknown,
    Integer,
    Boolean,
    Symbol(Cow<'ast, str>),
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
            TypingValue::Symbol(name) => write!(f, "{}", name),
            TypingValue::Boolean => write!(f, "a boolean"),
        }
    }
}
