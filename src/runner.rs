use crate::prelude::*;
use miette::MietteDiagnostic;
use std::{path::PathBuf, process::Command};

pub fn run(compiled: PathBuf) -> Result<String> {
    let output = Command::new(compiled)
        .output()
        .map_err(|e| vec![MietteDiagnostic::new(e.to_string())])?;

    if output.status.success() {
        let output = String::from_utf8(output.stdout)
            .map_err(|e| vec![MietteDiagnostic::new(e.to_string())])?;
        Ok(output)
    } else {
        let output = String::from_utf8(output.stderr)
            .map_err(|e| vec![MietteDiagnostic::new(e.to_string())])?;
        Err(vec![MietteDiagnostic::new(output)])
    }
}
