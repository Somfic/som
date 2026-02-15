mod error;

use std::{path::PathBuf, process::ExitStatus};

use crate::diagnostics::Diagnostic;
use error::RunnerError;

type Result<T> = std::result::Result<T, Diagnostic>;

pub struct Runner {
    executable: PathBuf,
    library_paths: Vec<PathBuf>,
}

impl Runner {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            library_paths: Vec::new(),
        }
    }

    pub fn with_library_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.library_paths = paths;
        self
    }

    pub fn run(&self) -> Result<ExitStatus> {
        let mut cmd = std::process::Command::new(&self.executable);

        // Set library search path for dynamic linker
        if !self.library_paths.is_empty() {
            let paths: Vec<_> = self.library_paths.iter().map(|p| p.as_os_str()).collect();
            let joined = std::env::join_paths(&paths).unwrap_or_default();

            if cfg!(target_os = "windows") {
                // Windows: prepend to PATH
                let current_path = std::env::var_os("PATH").unwrap_or_default();
                let new_path = std::env::join_paths(
                    paths
                        .iter()
                        .copied()
                        .chain(std::iter::once(current_path.as_os_str())),
                )
                .unwrap_or_default();
                cmd.env("PATH", new_path);
            } else if cfg!(target_os = "macos") {
                cmd.env("DYLD_LIBRARY_PATH", joined);
            } else {
                cmd.env("LD_LIBRARY_PATH", joined);
            }
        }

        cmd.status()
            .map_err(|err| RunnerError::ExecutionFailed(err).to_diagnostic())
    }
}
