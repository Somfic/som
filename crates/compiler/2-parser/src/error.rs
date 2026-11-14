use som_base::{Diagnostic, Fix, Label, Message, Span};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: Message,
    pub hint: Message,
    pub span: Span,
    /// Putting in what's missing, where it's missing: a `;`, a `)`.
    pub fix: Option<Fix>,
}

impl ParseError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        let d = Diagnostic::error(self.message.clone())
            .with_label(Label::primary(self.span.clone(), self.hint.clone()));
        match &self.fix {
            Some(fix) => d.with_fix(fix.clone()),
            None => d,
        }
    }
}
