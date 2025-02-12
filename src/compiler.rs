use miette::MietteDiagnostic;

use crate::prelude::*;
use std::path::PathBuf;

pub fn compile(source_code: &str) -> Result<PathBuf> {
    let lexer = lexer::Lexer::new(source_code);
    todo!()
}
