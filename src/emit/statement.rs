use crate::{
    ast::{Declaration, Scope, Statement},
    Emit, FunctionContext, ModuleContext, Result, Typed,
};

impl Emit for Statement<Typed> {
    type Output = ();

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        match self {
            Statement::Expression(expression) => expression.declare(ctx),
            Statement::Scope(scope) => scope.declare(ctx),
            Statement::Declaration(declaration) => declaration.declare(ctx),
        }
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        match self {
            Statement::Expression(e) => e.emit(ctx).map(|_| ())?,
            Statement::Scope(s) => s.emit(ctx)?,
            Statement::Declaration(d) => d.emit(ctx)?,
        };

        Ok(())
    }
}

impl Emit for Scope<Typed> {
    type Output = ();

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        for statement in &self.statements {
            statement.declare(ctx)?;
        }

        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        for statement in &self.statements {
            statement.emit(ctx)?;
        }

        Ok(())
    }
}

impl Emit for Declaration<Typed> {
    type Output = ();

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        self.value.declare(ctx)?;
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let value = self.value.emit(ctx)?;
        let var = ctx.declare_variable(self.name.clone(), self.value.ty().clone());
        ctx.builder.def_var(var, value);

        Ok(())
    }
}
