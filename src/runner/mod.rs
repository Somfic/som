use std::{path::PathBuf, process::ExitStatus};

use crate::diagnostics::Diagnostic;

type Result<T> = std::result::Result<T, Diagnostic>;

enum RunnerError {
    ExecutionFailed(std::io::Error),
}

impl RunnerError {
    fn to_diagnostic(self) -> Diagnostic {
        match self {
            RunnerError::ExecutionFailed(e) => {
                Diagnostic::error(format!("failed to execute: {}", e))
            }
        }
    }
}

pub struct Runner {
    executable: PathBuf,
}

impl Runner {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    pub fn run(&self) -> Result<ExitStatus> {
        std::process::Command::new(&self.executable)
            .status()
            .map_err(|err| RunnerError::ExecutionFailed(err).to_diagnostic())
    }
}
