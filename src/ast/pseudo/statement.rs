use std::fmt::format;

use crate::{
    ast::{Pseudo, Scope, Statement},
    Phase,
};

impl<P: Phase> Pseudo for Statement<P> {
    fn pseudo(&self) -> String {
        match self {
            Statement::Expression(e) => format!("{};", e.pseudo()),
            Statement::Scope(s) => format!("{{{}}}", s.pseudo()),
            Statement::FunctionDefinition(function_definition) => {
                let params = function_definition
                    .parameters
                    .iter()
                    .map(|p| format!("{} ~ {}", p.name, p.ty.pseudo()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "fn {}({}) -> {} {{ ... }}",
                    function_definition.name,
                    params,
                    function_definition.returns.pseudo()
                )
            }
            Statement::ValueDefinition(_) => format!("a variable declaration"),
            Statement::TypeDefinition(type_definition) => {
                format!(
                    "type {} = {};",
                    type_definition.name,
                    type_definition.ty.pseudo()
                )
            }
            Statement::ExternDefinition(extern_definition) => todo!(),
            Statement::WhileLoop(while_loop) => todo!(),
            Statement::Import(import) => format!("use {}", import.module),
        }
    }
}

impl<P: Phase> Pseudo for Scope<P> {
    fn pseudo(&self) -> String {
        self.statements
            .iter()
            .map(|s| format!("{}\n", s.pseudo()))
            .collect::<String>()
    }
}
