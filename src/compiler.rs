use miette::MietteDiagnostic;

use crate::{prelude::*, tokenizer};
use std::path::PathBuf;

pub fn compile(source_code: &str) -> Result<PathBuf> {
    let tokens = tokenizer::tokenize(source_code)?;
}
