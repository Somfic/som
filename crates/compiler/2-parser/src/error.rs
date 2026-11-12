use som_base::{Diagnostic, Label, Message, Span};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: Message,
    pub hint: Message,
    pub span: Span,
}

impl ParseError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::error(self.message.clone())
            .with_label(Label::primary(self.span.clone(), self.hint.clone()))
    }
}
