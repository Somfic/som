use crate::{ast::Expression, parser::ParsePhase, Phase, Result};

mod expr;

#[derive(Debug)]
pub struct TypeCheckPhase;

impl Phase for TypeCheckPhase {
    type TypeInfo = Type;
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
