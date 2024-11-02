use std::collections::HashSet;

use crate::parser::ast::Symbol;
use miette::{Report, Result, SourceSpan};

pub mod typing;

pub trait Passer {
    fn pass(ast: &Symbol<'_>) -> Result<PasserResult>;
}

#[derive(Default)]
pub struct PasserResult {
    pub non_critical: Vec<Report>,
    pub critical: Vec<Report>,
}

impl PasserResult {
    pub fn combine(mut self, other: PasserResult) -> Self {
        self.non_critical.extend(other.non_critical);
        self.critical.extend(other.critical);

        self
    }
}
