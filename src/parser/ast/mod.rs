use std::{borrow::Cow, fmt::Display};

use miette::SourceSpan;

pub mod typed;
pub mod untyped;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type<'de> {
    pub value: TypeValue<'de>,
    pub span: SourceSpan,
}

impl<'de> Type<'de> {
    pub fn label(&self, text: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, text.into())
    }

    pub fn unit(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Unit,
            span,
        }
    }

    pub fn boolean(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Boolean,
            span,
        }
    }

    pub fn integer(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Integer,
            span,
        }
    }

    pub fn decimal(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Decimal,
            span,
        }
    }

    pub fn character(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Character,
            span,
        }
    }

    pub fn string(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::String,
            span,
        }
    }

    pub fn symbol(span: SourceSpan, name: Cow<'de, str>) -> Self {
        Self {
            value: TypeValue::Symbol(name),
            span,
        }
    }

    pub fn collection(span: SourceSpan, element: Type<'de>) -> Self {
        Self {
            value: TypeValue::Collection(Box::new(element)),
            span,
        }
    }

    pub fn set(span: SourceSpan, element: Type<'de>) -> Self {
        Self {
            value: TypeValue::Set(Box::new(element)),
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeValue<'de> {
    Unit,
    Boolean,
    Integer,
    Decimal,
    Character,
    String,
    Symbol(Cow<'de, str>),
    Collection(Box<Type<'de>>),
    Set(Box<Type<'de>>),
}

impl<'de> TypeValue<'de> {
    pub fn matches(&self, other: &TypeValue<'de>) -> bool {
        match (&self, &other) {
            (TypeValue::Unit, TypeValue::Unit)
            | (TypeValue::Boolean, TypeValue::Boolean)
            | (TypeValue::Integer, TypeValue::Integer)
            | (TypeValue::Decimal, TypeValue::Decimal)
            | (TypeValue::Character, TypeValue::Character)
            | (TypeValue::String, TypeValue::String) => true,
            (TypeValue::Symbol(a), TypeValue::Symbol(b)) => a == b,
            (TypeValue::Collection(a), TypeValue::Collection(b)) => a.value.matches(&b.value),
            (TypeValue::Set(a), TypeValue::Set(b)) => a.value.matches(&b.value),
            _ => false,
        }
    }

    pub fn is_numeric(&self) -> bool {
        match self {
            TypeValue::Integer | TypeValue::Decimal => true,
            _ => false,
        }
    }

    pub fn is_primitive(&self) -> bool {
        match self {
            TypeValue::Unit
            | TypeValue::Boolean
            | TypeValue::Integer
            | TypeValue::Decimal
            | TypeValue::Character
            | TypeValue::String => true,
            _ => false,
        }
    }

    pub fn is_collection(&self) -> bool {
        match self {
            TypeValue::Collection(_) => true,
            _ => false,
        }
    }

    pub fn is_set(&self) -> bool {
        match self {
            TypeValue::Set(_) => true,
            _ => false,
        }
    }

    pub fn is_symbol(&self) -> bool {
        match self {
            TypeValue::Symbol(_) => true,
            _ => false,
        }
    }

    pub fn is_unit(&self) -> bool {
        match self {
            TypeValue::Unit => true,
            _ => false,
        }
    }

    pub fn is_boolean(&self) -> bool {
        match self {
            TypeValue::Boolean => true,
            _ => false,
        }
    }
}

impl Display for Type<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Display for TypeValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            TypeValue::Unit => write!(f, "nothing"),
            TypeValue::Boolean => write!(f, "a boolean"),
            TypeValue::Integer => write!(f, "an integer"),
            TypeValue::Decimal => write!(f, "a decimal"),
            TypeValue::Character => write!(f, "a character"),
            TypeValue::String => write!(f, "a string"),
            TypeValue::Symbol(name) => write!(f, "{}", name),
            TypeValue::Collection(element) => write!(f, "[{}]", element),
            TypeValue::Set(element) => write!(f, "{{{}}}", element),
        }
    }
}

pub trait Spannable<'de>: Sized {
    type Value;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self;

    fn at_multiple(spans: Vec<impl Into<miette::SourceSpan>>, value: Self::Value) -> Self {
        let spans = spans.into_iter().map(|s| s.into()).collect::<Vec<_>>();

        let start = spans
            .iter()
            .min_by_key(|s| s.offset())
            .map(|s| s.offset())
            .unwrap_or(0);

        let end = spans
            .iter()
            .max_by_key(|s| s.offset() + s.len())
            .map(|s| s.offset() + s.len())
            .unwrap_or(0);

        let span = miette::SourceSpan::new(start.into(), end - start);

        Self::at(span, value)
    }
}

pub trait CombineSpan {
    fn combine(spans: Vec<SourceSpan>) -> SourceSpan {
        let start = spans
            .iter()
            .min_by_key(|s| s.offset())
            .map(|s| s.offset())
            .unwrap_or(0);

        let end = spans
            .iter()
            .max_by_key(|s| s.offset() + s.len())
            .map(|s| s.offset() + s.len())
            .unwrap_or(0);

        SourceSpan::new(start.into(), end - start)
    }
}

impl CombineSpan for SourceSpan {}

impl<'de> Spannable<'de> for untyped::Expression<'de> {
    type Value = untyped::ExpressionValue<'de>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'de> Spannable<'de> for untyped::Statement<'de> {
    type Value = untyped::StatementValue<'de>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'de> Spannable<'de> for Type<'de> {
    type Value = TypeValue<'de>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}
