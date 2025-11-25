use crate::{
    ast::{
        Declaration, ExternDefinition, FunctionDefinition, Import, Scope, Statement,
        TypeDefinition, ValueDefinition, WhileLoop,
    },
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
            Statement::FunctionDefinition(function_definition) => function_definition.declare(ctx),
            Statement::ValueDefinition(declaration) => declaration.declare(ctx),
            Statement::TypeDefinition(type_definition) => type_definition.declare(ctx),
            Statement::ExternDefinition(extern_definition) => extern_definition.declare(ctx),
            Statement::WhileLoop(while_loop) => while_loop.declare(ctx),
            Statement::Import(import) => import.declare(ctx),
        }
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        match self {
            Statement::Expression(e) => e.emit(ctx).map(|_| ()),
            Statement::Scope(s) => s.emit(ctx),
            Statement::FunctionDefinition(f) => f.emit(ctx),
            Statement::ValueDefinition(d) => d.emit(ctx),
            Statement::TypeDefinition(type_definition) => type_definition.emit(ctx),
            Statement::ExternDefinition(extern_definition) => extern_definition.emit(ctx),
            Statement::WhileLoop(while_loop) => while_loop.emit(ctx),
            Statement::Import(import) => import.emit(ctx),
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

impl Emit for ValueDefinition<Typed> {
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
                    .function_registry
                    .get(lambda.id)
                    .ok_or_else(|| crate::EmitError::UndefinedFunction.to_diagnostic())?
                    .clone();

                // Compile the lambda body with self-name for recursion
                lambda.compile_body(
                    ctx.module,
                    ctx.function_registry,
                    func_id,
                    sig,
                    Some(self.name.name.to_string()),
                    ctx.global_functions,
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

impl Emit for Import {
    type Output = ();

    fn declare(&self, _ctx: &mut ModuleContext) -> Result<()> {
        todo!("handle imports at the module level")
    }

    fn emit(&self, _ctx: &mut FunctionContext) -> Result<Self::Output> {
        Ok(())
    }
}

impl Emit for FunctionDefinition<Typed> {
    type Output = ();

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        let mut sig = Signature::new(ctx.isa.default_call_conv());
        for param in &self.parameters {
            sig.params.push(AbiParam::new(param.ty.clone().into()));
        }

        sig.returns
            .push(AbiParam::new(self.returns.clone().into()));

        // Rename user's main to avoid conflict with C main wrapper
        let function_name = if self.name.to_string() == "main" {
            "_som_main".to_string()
        } else {
            self.name.to_string()
        };

        // Use Export linkage for public functions, Local for private
        let linkage = match self.visibility {
            crate::ast::Visibility::Public => Linkage::Export,
            _ => Linkage::Local,
        };

        let func_id = ctx
            .module
            .declare_function(&function_name, linkage, &sig)
            .map_err(|e| EmitError::ModuleError(e).to_diagnostic())?;

        ctx.function_registry.register(self.id, func_id, sig);

        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let (func_id, sig) = ctx
            .function_registry
            .get(self.id)
            .ok_or_else(|| crate::EmitError::UndefinedFunction.to_diagnostic())?
            .clone();

        self.compile_body(
            ctx.module,
            ctx.function_registry,
            func_id,
            sig,
            ctx.extern_registry,
            ctx.global_functions,
        )?;

        Ok(())
    }
}

impl FunctionDefinition<Typed> {
    pub fn compile_body(
        &self,
        module: &mut dyn cranelift::module::Module,
        function_registry: &crate::emit::FunctionRegistry,
        func_id: cranelift::module::FuncId,
        sig: cranelift::prelude::Signature,
        extern_registry: &std::collections::HashMap<String, (cranelift::module::FuncId, cranelift::prelude::Signature)>,
        global_functions: &std::collections::HashMap<String, usize>,
    ) -> Result<()> {
        use cranelift::codegen::ir::UserFuncName;
        use cranelift::prelude::FunctionBuilderContext;

        let mut cranelift_ctx = module.make_context();
        cranelift_ctx.func.signature = sig;
        cranelift_ctx.func.name = UserFuncName::user(0, func_id.as_u32());

        let mut builder_context = FunctionBuilderContext::new();
        let mut builder = cranelift::prelude::FunctionBuilder::new(
            &mut cranelift_ctx.func,
            &mut builder_context,
        );

        let entry_block = builder.create_block();

        // Add block parameters for each function parameter
        for param in &self.parameters {
            builder.append_block_param(entry_block, param.ty.clone().into());
        }

        builder.switch_to_block(entry_block);

        let param_values: Vec<_> = (0..self.parameters.len())
            .map(|i| builder.block_params(entry_block)[i])
            .collect();

        let mut func_ctx = FunctionContext::new(
            &mut builder,
            module,
            function_registry,
            extern_registry,
            global_functions,
        );

        // Register the function itself so it can be called recursively
        func_ctx
            .self_referencing_lambdas
            .insert(self.name.to_string(), self.id);

        // Declare parameters as variables so they can be referenced in the body
        for (param, &cranelift_param) in self.parameters.iter().zip(&param_values) {
            let var = func_ctx.declare_variable(param.name.name.to_string(), param.ty.clone());
            func_ctx.builder.def_var(var, cranelift_param);
        }

        // Emit the function body
        let return_val = self.body.emit(&mut func_ctx)?;
        func_ctx.builder.ins().return_(&[return_val]);
        func_ctx.builder.seal_all_blocks();

        module
            .define_function(func_id, &mut cranelift_ctx)
            .map_err(|e| EmitError::ModuleError(e).to_diagnostic())?;

        module.clear_context(&mut cranelift_ctx);
        Ok(())
    }
}
