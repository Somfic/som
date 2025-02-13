use crate::{ast::TypedExpression, prelude::*};
use jit::Jit;
use std::path::PathBuf;

pub mod jit;

pub struct Compiler<'ast> {
    jit: Jit,
    expression: TypedExpression<'ast>,
}

impl<'ast> Compiler<'ast> {
    pub fn new(expression: TypedExpression<'ast>) -> Self {
        Self {
            jit: Jit::default(),
            expression,
        }
    }

    pub fn compile(&mut self) -> Result<PathBuf> {
        Ok(PathBuf::new())
    }
}
