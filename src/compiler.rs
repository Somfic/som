use miette::MietteDiagnostic;

use crate::{
    parser::{BindingPower, Parser},
    prelude::*,
    tokenizer::{self, TokenKind, Tokenizer},
    typer::{self, Typer},
};
use std::path::PathBuf;

pub fn compile(source_code: &str) -> Result<PathBuf> {
    let mut parser = Parser::new(source_code);

    let expression = parser.parse_expression(BindingPower::None)?;

    let mut typer = Typer::new();
    let expression = typer.type_check(expression)?;

    println!("{:?}", expression);

    Ok(PathBuf::new())
}
