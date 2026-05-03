use crate::Span;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<&'static str>,
    pub message: String,
    pub primary: Label,
    pub secondary: Vec<Label>,
    pub suggestions: Vec<Suggestion>,
    pub notes: Vec<String>,
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

    pub fn emit_error(&mut self, span: Span, message: impl Into<String>) {
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
