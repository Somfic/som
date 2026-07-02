use crate::{Message, Span};

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<&'static str>,
    pub message: Message,
    pub primary: Label,
    pub secondary: Vec<Label>,
    pub suggestions: Vec<Suggestion>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Start a diagnostic anchored at `span`. The primary label starts with an
    /// empty message; add one with [`Diagnostic::label`].
    pub fn new(severity: Severity, span: Span, message: impl Into<Message>) -> Self {
        Self {
            severity,
            code: None,
            message: message.into(),
            primary: Label {
                span,
                message: String::new(),
            },
            secondary: Vec::new(),
            suggestions: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn error(span: Span, message: impl Into<Message>) -> Self {
        Self::new(Severity::Error, span, message)
    }

    pub fn warning(span: Span, message: impl Into<Message>) -> Self {
        Self::new(Severity::Warning, span, message)
    }

    /// Set the message shown under the primary underline.
    pub fn label(mut self, message: impl Into<String>) -> Self {
        self.primary.message = message.into();
        self
    }

    /// Add a secondary underline elsewhere in the source.
    pub fn secondary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.secondary.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Attach a free-standing note printed below the snippet.
    pub fn note(mut self, message: impl Into<String>) -> Self {
        self.notes.push(message.into());
        self
    }

    /// Propose an edit at `span`.
    pub fn suggest(
        mut self,
        span: Span,
        replacement: impl Into<String>,
        message: impl Into<String>,
        applicability: Applicability,
    ) -> Self {
        self.suggestions.push(Suggestion {
            span,
            replacement: replacement.into(),
            message: message.into(),
            applicability,
        });
        self
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub span: Span,
    pub replacement: String,
    pub message: String,
    pub applicability: Applicability,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Applicability {
    /// Safe to apply automatically. e.g. inserting a missing `;`.
    MachineApplicable,
    /// Probably right but the user should review. e.g. a typo correction.
    MaybeIncorrect,
    /// Has placeholders or requires human judgment.
    HasPlaceholders,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

#[derive(Default, Debug)]
pub struct DiagnosticSink {
    diagnostics: Vec<Diagnostic>,
    error_count: u32,
    warning_count: u32,
}

impl DiagnosticSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn emit(&mut self, diag: Diagnostic) {
        match diag.severity {
            Severity::Error => self.error_count += 1,
            Severity::Warning => self.warning_count += 1,
            _ => {}
        }
        self.diagnostics.push(diag);
    }

    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }
    pub fn error_count(&self) -> u32 {
        self.error_count
    }
    pub fn warning_count(&self) -> u32 {
        self.warning_count
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn finalize(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn emit_error(&mut self, span: Span, message: impl Into<Message>) {
        self.emit(Diagnostic {
            severity: Severity::Error,
            code: None,
            message: message.into(),
            primary: Label {
                span,
                message: String::new(),
            },
            secondary: Vec::new(),
            suggestions: Vec::new(),
            notes: Vec::new(),
        });
    }
}
