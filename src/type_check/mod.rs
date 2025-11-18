use crate::{
    ast::{Expression, Type},
    parser::Untyped,
    Phase, Result, TypeCheckError,
};
use std::collections::HashMap;

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

pub struct TypeCheckContext<'a> {
    parent: Option<&'a TypeCheckContext<'a>>,
    variables: HashMap<String, Type>,
}

impl<'a> TypeCheckContext<'a> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
        }
    }

    pub fn get_variable(&self, name: impl Into<String>) -> Result<Type> {
        let name = name.into();

        self.variables
            .get(&name)
            .cloned()
            .or_else(|| {
                self.parent
                    .and_then(|parent_ctx| parent_ctx.get_variable(name).ok())
            })
            .ok_or_else(|| TypeCheckError::UndefinedVariable.to_diagnostic())
    }

    pub fn declare_variable(&mut self, name: impl Into<String>, ty: Type) {
        self.variables.insert(name.into(), ty);
    }

    fn new_child_context(&self) -> TypeCheckContext<'_> {
        TypeCheckContext {
            parent: Some(self),
            variables: HashMap::new(),
        }
    }
}
