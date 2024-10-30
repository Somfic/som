use crate::parser::ast::Symbol;
use miette::{Report, Result};

pub mod typing;

pub trait Passer {
    fn pass(ast: &Symbol<'_>) -> Result<PasserResult>;
}

pub struct PasserResult {
    pub non_critical: Vec<Report>,
    pub critical: Vec<Report>,
}
