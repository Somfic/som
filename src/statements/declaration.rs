use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct DeclarationStatement<Expression> {
    pub identifier: Identifier,
    pub explicit_type: Option<Type>,
    pub value: Box<Expression>,
}

pub fn
