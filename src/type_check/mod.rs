use std::collections::HashMap;

use cranelift::prelude::types;

use crate::{ast::Expression, parser::Untyped, Phase, Result, TypeCheckError};

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
        expression.type_check(&mut TypeCheckContext::new())
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

impl From<Type> for cranelift::prelude::Type {
    fn from(val: Type) -> Self {
        match val {
            Type::Unit => types::I8,
            Type::Boolean => types::I8,
            Type::I32 => types::I32,
            Type::I64 => types::I64,
            Type::Decimal => types::F64,
            Type::String => todo!(),
            Type::Character => todo!(),
        }
    }
}

pub struct TypeCheckContext {
    variables: HashMap<String, Type>,
}

impl TypeCheckContext {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    pub fn get_variable(&self, name: impl Into<String>) -> Result<Type> {
        let name = name.into();

        self.variables
            .get(&name)
            .cloned()
            .ok_or_else(|| TypeCheckError::UndefinedVariable(name.clone()).to_diagnostic())
    }

    pub fn declare_variable(&mut self, name: impl Into<String>, ty: Type) {
        self.variables.insert(name.into(), ty);
    }
}
