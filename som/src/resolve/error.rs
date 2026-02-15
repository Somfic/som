use crate::diagnostics::{Highlight, Label};
use crate::{Diagnostic, Span};

#[derive(Clone, Debug)]
pub enum ResolveError {
    UnresolvedVariable {
        name: String,
        span: Span,
    },
    UnresolvedFunction {
        name: String,
        span: Span,
    },
    CannotAccessOuterLocal {
        name: String,
        span: Span,
    },
}

impl ResolveError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            ResolveError::UnresolvedVariable { name, span } => {
                Diagnostic::error(format!("cannot find variable {} in this scope", name.as_var()))
                    .with_label(Label::primary(span.clone(), "not found in this scope"))
            }

            ResolveError::UnresolvedFunction { name, span } => {
                Diagnostic::error(format!(
                    "cannot find function {} in this scope",
                    name.as_func()
                ))
                .with_label(Label::primary(span.clone(), "not found in this scope"))
            }

            ResolveError::CannotAccessOuterLocal { name, span } => {
                Diagnostic::error(format!(
                    "cannot capture variable {} from outer scope",
                    name.as_var()
                ))
                .with_label(Label::primary(
                    span.clone(),
                    "functions cannot capture variables",
                ))
            }
        }
    }
}
