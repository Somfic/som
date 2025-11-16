use crate::{
    ast::{Scope, Statement},
    Emit, Result, Typed,
};

impl Emit for Statement<Typed> {
    type Output = ();

    fn emit(&self, ctx: &mut super::EmitContext) -> Result<Self::Output> {
        match self {
            Statement::Expression(expression) => {
                expression.emit(ctx)?;
            }
            Statement::Scope(scope) => {
                scope.emit(ctx)?;
            }
        };

        Ok(())
    }
}

impl Emit for Scope<Typed> {
    type Output = ();

    fn emit(&self, ctx: &mut super::EmitContext) -> Result<Self::Output> {
        for statement in &self.statements {
            statement.emit(ctx)?;
        }

        Ok(())
    }
}
