use crate::{LinkerError, Result};
use std::{path::PathBuf, process::Command};

pub struct Linker {
    pub output: String,
}

impl Linker {
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
        }
    }

    pub fn link_modules(&self, modules: Vec<PathBuf>) -> Result<PathBuf> {
        let status = Command::new(find_linker()?)
            .arg("-o")
            .arg(self.output.clone())
            .args(modules)
            .status()
            .map_err(|err| LinkerError::IoError(err).to_diagnostic())?;

        if !status.success() {
            return Err(LinkerError::FailedToLink.to_diagnostic());
        }

        Ok(PathBuf::from(self.output.clone()))
    }
}

fn find_linker() -> Result<String> {
    for compiler in &["cc", "gcc", "clang", "zig cc"] {
        if std::process::Command::new(compiler)
            .arg("--version")
            .output()
            .is_ok()
        {
            return Ok(compiler.to_string());
        }
    }

    Err(LinkerError::NoLinkerFound.to_diagnostic())
}
