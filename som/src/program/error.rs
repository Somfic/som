use std::{io, path::PathBuf};

use crate::{Diagnostic, Label, diagnostics::Highlight, parser::ParseError};

#[derive(Debug)]
pub enum ProgramError {
    Io(io::Error),
    CircularDependency(String),
    ModuleNotFound {
        name: String,
        path: PathBuf,
        span: Option<crate::Span>,
    },
}

impl ProgramError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            ProgramError::Io(err) => Diagnostic::error(format!("I/O error: {}", err)),
            ProgramError::CircularDependency(module) => Diagnostic::error(format!(
                "circular dependency detected while loading module {}",
                module.as_module()
            )),
            ProgramError::ModuleNotFound { name, path, span } => {
                let diag =
                    Diagnostic::error(format!("module {} could not be found", name.as_module()));
                match span {
                    Some(s) => diag
                        .with_label(Label::primary(s.clone(), "unknown module"))
                        .with_hint(format!("cannot find source code in {}", path.display())),
                    None => diag.with_hint(format!("expected at {}", path.display())),
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct LoadErrors {
    pub program: Vec<ProgramError>,
    pub parse: Vec<ParseError>,
}
