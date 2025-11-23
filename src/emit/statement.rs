use crate::{
    ast::{Declaration, ExternDefinition, Scope, Statement, TypeDefinition, WhileLoop},
    Emit, EmitError, FunctionContext, ModuleContext, Result, Typed,
};
use cranelift::{
    module::Linkage,
    prelude::{types, AbiParam, InstBuilder, Signature},
};

impl Emit for Statement<Typed> {
    type Output = ();

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        match self {
            Statement::Expression(expression) => expression.declare(ctx),
            Statement::Scope(scope) => scope.declare(ctx),
            Statement::Declaration(declaration) => declaration.declare(ctx),
            Statement::TypeDefinition(type_definition) => type_definition.declare(ctx),
            Statement::ExternDefinition(extern_definition) => extern_definition.declare(ctx),
            Statement::WhileLoop(while_loop) => while_loop.declare(ctx),
        }
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        match self {
            Statement::Expression(e) => e.emit(ctx).map(|_| ()),
            Statement::Scope(s) => s.emit(ctx),
            Statement::Declaration(d) => d.emit(ctx),
            Statement::TypeDefinition(type_definition) => type_definition.emit(ctx),
            Statement::ExternDefinition(extern_definition) => extern_definition.emit(ctx),
            Statement::WhileLoop(while_loop) => while_loop.emit(ctx),
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

impl Emit for ExternDefinition {
    type Output = ();

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        for function in &self.functions {
            let mut sig = Signature::new(ctx.isa.default_call_conv());
            for param in &function.signature.parameters {
                sig.params.push(AbiParam::new(param.clone().into()));
            }

            sig.returns
                .push(AbiParam::new((*function.signature.returns).clone().into()));

            let func_id = ctx
                .module
                .declare_function(&function.symbol.to_string(), Linkage::Import, &sig)
                .map_err(|e| EmitError::ModuleError(e).to_diagnostic())?;

            ctx.extern_registry
                .insert(function.name.name.to_string(), (func_id, sig));
        }
        Ok(())
    }

    fn emit(&self, _ctx: &mut FunctionContext) -> Result<Self::Output> {
        Ok(())
    }
}

impl Emit for WhileLoop<Typed> {
    type Output = ();

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let loop_block = ctx.builder.create_block();
        let after_block = ctx.builder.create_block();

        ctx.builder.ins().jump(loop_block, &[]);

        ctx.builder.switch_to_block(loop_block);
        let condition_value = self.condition.emit(ctx)?;

        let body_block = ctx.builder.create_block();
        ctx.builder
            .ins()
            .brif(condition_value, body_block, &[], after_block, &[]);

        ctx.builder.switch_to_block(body_block);
        self.statement.emit(ctx)?;

        ctx.builder.ins().jump(loop_block, &[]);

        ctx.builder.switch_to_block(after_block);

        Ok(())
    }
}
