use crate::diagnostics::Diagnostic;

pub(super) enum LinkerError {
    NoLinkerFound,
    FailedToLink { stderr: String },
    IoError(std::io::Error),
}

impl LinkerError {
    pub(super) fn to_diagnostic(self) -> Diagnostic {
        match self {
            LinkerError::NoLinkerFound => Diagnostic::error("no linker found"),
            LinkerError::FailedToLink { stderr } => {
                let mut diag = Diagnostic::error("failed to link");
                for line in stderr.lines() {
                    if !line.is_empty() {
                        diag = diag.with_trace(line);
                    }
                }
                diag
            }
            LinkerError::IoError(e) => Diagnostic::error(format!("io error: {}", e)),
        }
    }
}
