use miette::SourceSpan;
use std::{borrow::Cow, fmt::Display};

#[derive(Debug, Clone)]
pub struct Type<'ast> {
    pub value: TypeValue<'ast>,
    pub span: SourceSpan,
}

impl<'ast> Type<'ast> {
    pub fn label(&self, text: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, text.into())
    }

    pub fn unit(span: &SourceSpan) -> Self {
        Self {
            value: TypeValue::Unit,
            span: *span,
        }
    }

    pub fn boolean(span: &SourceSpan) -> Self {
        Self {
            value: TypeValue::Boolean,
            span: *span,
        }
    }

    pub fn integer(span: &SourceSpan) -> Self {
        Self {
            value: TypeValue::Integer,
            span: *span,
        }
    }

    pub fn decimal(span: &SourceSpan) -> Self {
        Self {
            value: TypeValue::Decimal,
            span: *span,
        }
    }

    pub fn character(span: &SourceSpan) -> Self {
        Self {
            value: TypeValue::Character,
            span: *span,
        }
    }

    pub fn string(span: &SourceSpan) -> Self {
        Self {
            value: TypeValue::String,
            span: *span,
        }
    }

    pub fn symbol(span: &SourceSpan, name: Cow<'ast, str>) -> Self {
        Self {
            value: TypeValue::Symbol(name),
            span: *span,
        }
    }

    pub fn collection(span: &SourceSpan, element: Type<'ast>) -> Self {
        Self {
            value: TypeValue::Collection(Box::new(element)),
            span: *span,
        }
    }

    pub fn set(span: &SourceSpan, element: Type<'ast>) -> Self {
        Self {
            value: TypeValue::Set(Box::new(element)),
            span: *span,
        }
    }

    pub fn function(
        span: &SourceSpan,
        parameters: Vec<Type<'ast>>,
        return_type: Type<'ast>,
    ) -> Self {
        Self {
            value: TypeValue::Function {
                parameters,
                return_type: Box::new(return_type),
            },
            span: *span,
        }
    }

    pub fn alias(span: &SourceSpan, name: Cow<'ast, str>, alias: Type<'ast>) -> Self {
        Self {
            value: TypeValue::Alias(name, Box::new(alias)),
            span: *span,
        }
    }

    pub fn span(mut self, span: SourceSpan) -> Self {
        self.span = span;
        self
    }

    pub fn base_type(&self) -> &Type<'ast> {
        if let TypeValue::Alias(_, alias) = &self.value {
            return alias.base_type();
        };

        self
    }
}

impl Eq for Type<'_> {}
impl PartialEq for Type<'_> {
    fn eq(&self, other: &Self) -> bool {
        let a = self.base_type();
        let b = other.base_type();

        a.value.eq(&b.value)
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
