use crate::{
    ast::{Pseudo, Scope, Statement},
    Phase,
};

impl<P: Phase> Pseudo for Statement<P> {
    fn pseudo(&self) -> String {
        match self {
            Statement::Expression(e) => format!("{};", e.pseudo()),
            Statement::Scope(s) => format!("{{{}}}", s.pseudo()),
            Statement::Declaration(_) => format!("a variable declaration"),
            Statement::TypeDefinition(type_definition) => {
                format!(
                    "type {} = {};",
                    type_definition.name,
                    type_definition.ty.pseudo()
                )
            }
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
