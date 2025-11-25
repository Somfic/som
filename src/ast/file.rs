use std::fmt::Display;

use crate::{
    ast::{ExternDefinition, FunctionDefinition, Import, TypeDefinition, ValueDefinition},
    Phase,
};

#[derive(Debug)]
pub struct File<P: Phase> {
    pub declarations: Vec<Declaration<P>>,
}

#[derive(Debug)]
pub enum Declaration<P: Phase> {
    Import(Import),
    TypeDefinition(TypeDefinition),
    FunctionDefinition(FunctionDefinition<P>),
    ExternDefinition(ExternDefinition),
}

impl<P: Phase> Display for Declaration<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Declaration::Import(import) => write!(f, "an import"),
            Declaration::FunctionDefinition(function_definition) => {
                write!(f, "a function definition")
            }
            Declaration::TypeDefinition(type_definition) => write!(f, "a type definition"),
            Declaration::ExternDefinition(extern_definition) => write!(f, "an extern definition"),
        }
    }
}
