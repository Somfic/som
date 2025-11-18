use crate::{Result, RunnerError};
use std::{path::PathBuf, process::ExitStatus};

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
            .map_err(|err| RunnerError::ExecutionFailed.to_diagnostic())
    }
}
