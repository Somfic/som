use crate::Span;
use crate::diagnostics::{Diagnostic, Label};

/// Parse error with location information
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub hint: String,
    pub span: Span,
}

impl ParseError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::error(&self.message).with_label(Label::primary(self.span.clone(), &self.hint))
    }
}
