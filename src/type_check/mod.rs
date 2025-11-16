use crate::{ast::Expression, parser::Untyped, Phase, Result};

mod expr;

#[derive(Debug)]
pub struct Typed;

impl Phase for Typed {
    type TypeInfo = Type;
}

pub struct Typer {}

impl Typer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn check(&mut self, expression: Expression<Untyped>) -> Result<Expression<Typed>> {
        expression
            .type_check(&mut TypeCheckContext {})
            .map(|(e, _)| e)
    }
}

pub trait TypeCheck: Sized {
    type Output;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)>;
}

#[derive(Debug, Clone)]
pub enum Type {
    Boolean,
    I32,
    I64,
    Decimal,
    String,
    Character,
}

pub struct TypeCheckContext {}
