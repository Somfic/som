use std::collections::HashSet;

use crate::{diagnostic::Diagnostic, parser::ast::Symbol};

pub fn check(ast: &Symbol) -> Result<(), HashSet<Diagnostic>> {
    Ok(())
}
