use crate::{
    ast::{Declaration, Scope, Statement},
    Emit, Result, Typed,
};

impl Emit for Statement<Typed> {
    type Output = ();

    fn emit(&self, ctx: &mut super::EmitContext) -> Result<Self::Output> {
        match self {
            Statement::Expression(e) => {
                e.emit(ctx)?;
            }
            Statement::Scope(s) => {
                s.emit(ctx)?;
            }
            Statement::Declaration(d) => {
                d.emit(ctx)?;
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

impl Emit for Declaration<Typed> {
    type Output = ();

    fn emit(&self, ctx: &mut super::EmitContext) -> Result<Self::Output> {
        let value = self.value.emit(ctx)?;
        let var = ctx.declare_variable(self.name.clone(), self.value.ty().clone());
        ctx.builder.def_var(var, value);

        Ok(())
    }
}
