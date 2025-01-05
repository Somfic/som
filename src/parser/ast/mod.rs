use miette::SourceSpan;
use std::{borrow::Cow, fmt::Display};

pub mod typed;
pub mod untyped;

#[derive(Debug, Clone)]
pub struct Type<'ast> {
    pub value: TypeValue<'ast>,
    pub span: SourceSpan,
    pub original_span: Option<SourceSpan>,
}

impl<'ast> Type<'ast> {
    pub fn label(&self, text: impl Into<String>) -> Vec<miette::LabeledSpan> {
        let labels = vec![miette::LabeledSpan::at(self.span, text.into())];

        if let Some(_original_span) = self.original_span {
            // labels.push(miette::LabeledSpan::at(
            //     original_span,
            //     "original type declaration".to_string(),
            // ));
        }

        labels
    }

    pub fn unit(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Unit,
            span,
            original_span: None,
        }
    }

    pub fn boolean(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Boolean,
            span,
            original_span: None,
        }
    }

    pub fn integer(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Integer,
            span,
            original_span: None,
        }
    }

    pub fn decimal(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Decimal,
            span,
            original_span: None,
        }
    }

    pub fn character(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::Character,
            span,
            original_span: None,
        }
    }

    pub fn string(span: SourceSpan) -> Self {
        Self {
            value: TypeValue::String,
            span,
            original_span: None,
        }
    }

    pub fn symbol(span: SourceSpan, name: Cow<'ast, str>) -> Self {
        Self {
            value: TypeValue::Symbol(name),
            span,
            original_span: None,
        }
    }

    pub fn collection(span: SourceSpan, element: Type<'ast>) -> Self {
        Self {
            value: TypeValue::Collection(Box::new(element)),
            span,
            original_span: None,
        }
    }

    pub fn set(span: SourceSpan, element: Type<'ast>) -> Self {
        Self {
            value: TypeValue::Set(Box::new(element)),
            span,
            original_span: None,
        }
    }

    pub fn function(
        span: SourceSpan,
        parameters: Vec<Type<'ast>>,
        return_type: Type<'ast>,
    ) -> Self {
        Self {
            value: TypeValue::Function {
                parameters,
                return_type: Box::new(return_type),
            },
            span,
            original_span: None,
        }
    }

    pub fn alias(span: SourceSpan, name: Cow<'ast, str>, alias: Type<'ast>) -> Self {
        Self {
            value: TypeValue::Alias(name, Box::new(alias)),
            span,
            original_span: None,
        }
    }

    pub fn span(mut self, span: SourceSpan) -> Self {
        if self.original_span.is_none() {
            self.original_span = Some(self.span);
        }
        self.span = span;
        self
    }
}

impl Eq for Type<'_> {}
impl PartialEq for Type<'_> {
    fn eq(&self, other: &Self) -> bool {
        if let TypeValue::Alias(_, a) = &self.value {
            return a.value.eq(&other.value);
        }

        if let TypeValue::Alias(_, b) = &other.value {
            return self.value.eq(&b.value);
        }

        self.value.eq(&other.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeValue<'ast> {
    Unit,
    Boolean,
    Integer,
    Decimal,
    Character,
    String,
    Alias(Cow<'ast, str>, Box<Type<'ast>>),
    Symbol(Cow<'ast, str>),
    Collection(Box<Type<'ast>>),
    Set(Box<Type<'ast>>),
    Function {
        parameters: Vec<Type<'ast>>,
        return_type: Box<Type<'ast>>,
    },
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
            TypeValue::Symbol(name) => write!(f, "`{}`", name),
            TypeValue::Collection(element) => write!(f, "[{}]", element),
            TypeValue::Set(element) => write!(f, "{{{}}}", element),
            TypeValue::Function {
                parameters,
                return_type,
            } => {
                write!(
                    f,
                    "fn ({})",
                    parameters
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                )?;

                if return_type.value != TypeValue::Unit {
                    write!(f, " -> {}", return_type)?;
                }

                Ok(())
            }
            TypeValue::Alias(name, alias) => write!(f, "`{}` type alias with type {}", name, alias),
        }
    }
}

pub trait Spannable<'ast>: Sized {
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

impl<'ast> Spannable<'ast> for untyped::Expression<'ast> {
    type Value = untyped::ExpressionValue<'ast>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'ast> Spannable<'ast> for untyped::Statement<'ast> {
    type Value = untyped::StatementValue<'ast>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'ast> Spannable<'ast> for Type<'ast> {
    type Value = TypeValue<'ast>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self {
            value,
            span,
            original_span: None,
        }
    }
}
