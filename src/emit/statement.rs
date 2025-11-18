use crate::{
    ast::{Declaration, Scope, Statement, TypeDefinition},
    Emit, FunctionContext, ModuleContext, Result, Typed,
};
use cranelift::prelude::{types, InstBuilder};

impl Emit for Statement<Typed> {
    type Output = ();

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        match self {
            Statement::Expression(expression) => expression.declare(ctx),
            Statement::Scope(scope) => scope.declare(ctx),
            Statement::Declaration(declaration) => declaration.declare(ctx),
            Statement::TypeDefinition(type_definition) => type_definition.declare(ctx),
        }
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        match self {
            Statement::Expression(e) => e.emit(ctx).map(|_| ()),
            Statement::Scope(s) => s.emit(ctx),
            Statement::Declaration(d) => d.emit(ctx),
            Statement::TypeDefinition(type_definition) => type_definition.emit(ctx),
        }
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
        use crate::ast::Expression;

        let value = match &*self.value {
            // Special handling for lambdas to support recursion
            Expression::Lambda(lambda) => {
                let (func_id, sig) = ctx
                    .lambda_registry
                    .get(lambda.id)
                    .ok_or_else(|| crate::EmitError::UndefinedFunction.to_diagnostic())?
                    .clone();

                // Compile the lambda body with self-name for recursion
                lambda.compile_body(
                    ctx.module,
                    ctx.lambda_registry,
                    func_id,
                    sig,
                    Some(self.name.name.to_string()),
                )?;

                // Return function address
                let reference = ctx.module.declare_func_in_func(func_id, ctx.builder.func);
                ctx.builder.ins().func_addr(types::I64, reference)
            }
            _ => self.value.emit(ctx)?,
        };

        let var = ctx.declare_variable(self.name.clone(), self.value.ty().clone());
        ctx.builder.def_var(var, value);

        Ok(())
    }
}

impl Emit for TypeDefinition {
    type Output = ();

    fn declare(&self, _ctx: &mut ModuleContext) -> Result<()> {
        Ok(())
    }

    fn emit(&self, _ctx: &mut FunctionContext) -> Result<Self::Output> {
        Ok(())
    }
}
