use crate::{
    ast::{Expression, Statement},
    parser::Untyped,
    Phase, Result,
};

mod expression;
mod statement;

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

    // pub fn check(&mut self, expression: Expression<Untyped>) -> Result<Expression<Typed>> {
    //     expression
    //         .type_check(&mut TypeCheckContext {})
    //         .map(|(e, _)| e)
    // }

    pub fn check(&mut self, expression: Expression<Untyped>) -> Result<Expression<Typed>> {
        expression.type_check(&mut TypeCheckContext {})
    }
}

pub trait TypeCheck: Sized {
    type Output;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output>;
}

pub trait TypeCheckWithType: Sized {
    type Output;

    fn type_check_with_type(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)>;
}

#[derive(Debug, Clone)]
pub enum Type {
    Unit,
    Boolean,
    I32,
    I64,
    Decimal,
    String,
    Character,
}

pub struct TypeCheckContext {}
