use crate::{
    ast::{Pseudo, Scope, Statement},
    Phase,
};

impl<P: Phase> Pseudo for Statement<P> {
    fn pseudo(&self) -> String {
        match self {
            Statement::Expression(e) => format!("{};", e.pseudo()),
            Statement::Scope(s) => format!("{{{}}}", s.pseudo()),
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
