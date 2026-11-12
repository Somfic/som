use som_base::message::{Message, Part, func, struct_name, ty, var};
use som_base::{Diagnostic, Label, Span, msg};

#[derive(Debug, Clone)]
pub enum ResolveError {
    UnboundVariable {
        name: String,
        span: Span,
        help: Option<Message>,
    },
    UnknownFunction {
        name: String,
        span: Span,
        help: Option<Message>,
    },
    UnknownStruct {
        name: String,
        span: Span,
        help: Option<Message>,
    },
    UnknownType {
        name: String,
        span: Span,
        help: Option<Message>,
    },
    DuplicateDefinition {
        name: String,
        span: Span,
        previous: Span,
    },
}

impl ResolveError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        let (what, name, span, help): (&str, Part, &Span, &Option<Message>) = match self {
            ResolveError::UnboundVariable { name, span, help } => {
                ("variable", var(name), span, help)
            }
            ResolveError::UnknownFunction { name, span, help } => {
                ("function", func(name), span, help)
            }
            ResolveError::UnknownStruct { name, span, help } => {
                ("struct", struct_name(name), span, help)
            }
            ResolveError::UnknownType { name, span, help } => ("type", ty(name), span, help),
            ResolveError::DuplicateDefinition {
                name,
                span,
                previous,
            } => {
                return Diagnostic::error(msg![var(name), " is defined more than once"])
                    .with_label(Label::primary(span.clone(), "redefined here"))
                    .with_label(Label::secondary(previous.clone(), "first defined here"));
            }
        };

        let mut diagnostic = Diagnostic::error(msg!["cannot find ", what, " ", name])
            .with_label(Label::primary(span.clone(), "not found in this scope"));
        if let Some(help) = help {
            diagnostic = diagnostic.with_hint(help.clone());
        }
        diagnostic
    }
}
