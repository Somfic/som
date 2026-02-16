use crate::diagnostics::Diagnostic;

pub(super) enum RunnerError {
    ExecutionFailed(std::io::Error),
}

impl RunnerError {
    pub(super) fn to_diagnostic(self) -> Diagnostic {
        match self {
            RunnerError::ExecutionFailed(e) => {
                Diagnostic::error(format!("failed to execute: {}", e))
            }
        }
    }
}
