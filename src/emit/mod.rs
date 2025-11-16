use crate::{ast::Expression, EmitError, Result, Typed};
use cranelift::{
    codegen::ir::UserFuncName,
    jit::{JITBuilder, JITModule},
    module::{self, FuncId, Linkage, Module},
    prelude::{settings::Flags, *},
};
use std::{collections::HashMap, sync::Arc};
use target_lexicon::Triple;

mod expression;
mod statement;

pub trait Emit {
    type Output;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output>;
}

pub struct Emitter {
    pub isa: Arc<dyn isa::TargetIsa>,
    module: JITModule, // todo: switch to ObjectModule
}

impl Emitter {
    pub fn new(triple: Triple) -> Self {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();

        let isa = isa::lookup(triple)
            .unwrap()
            .finish(Flags::new(flag_builder))
            .unwrap();

        let module_builder = JITBuilder::with_isa(isa.clone(), module::default_libcall_names());
        let module = JITModule::new(module_builder);

        Self { isa, module }
    }

    pub fn compile(&mut self, expression: &Expression<Typed>) -> Result<*const u8> {
        let mut lambda_registry = LambdaRegistry::new();

        {
            let mut module_ctx =
                ModuleContext::new(self.isa.clone(), &mut self.module, &mut lambda_registry);
            expression.declare(&mut module_ctx)?;
        }

        // wrap the expression in the main function
        let mut main_signature = Signature::new(self.isa.default_call_conv());
        main_signature.params = vec![];
        main_signature.returns = vec![AbiParam::new(types::I64)];

        let main_id = self.compile_function(
            "main",
            main_signature,
            Linkage::Export,
            &mut lambda_registry,
            |func_ctx| {
                let value = expression.emit(func_ctx)?;
                // cast if the value is not already i64
                let value_type = func_ctx.builder.func.dfg.value_type(value);
                let value = if value_type != types::I64 {
                    func_ctx.builder.ins().sextend(types::I64, value)
                } else {
                    value
                };
                Ok(value)
            },
        )?;

        self.module
            .finalize_definitions()
            .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?;

        Ok(self.module.get_finalized_function(main_id))
    }

    fn compile_function<F>(
        &mut self,
        name: &str,
        sig: Signature,
        linkage: Linkage,
        lambda_registry: &mut LambdaRegistry,
        emit_body: F,
    ) -> Result<FuncId>
    where
        F: FnOnce(&mut FunctionContext) -> Result<Value>,
    {
        let func_id = self
            .module
            .declare_function(name, linkage, &sig)
            .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?;

        let mut cranelift_ctx = self.module.make_context();
        cranelift_ctx.func.signature = sig;
        cranelift_ctx.func.name = UserFuncName::user(0, func_id.as_u32());

        let mut builder_context = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut cranelift_ctx.func, &mut builder_context);

        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);

        {
            let mut func_ctx =
                FunctionContext::new(&mut builder, &mut self.module, lambda_registry);
            let return_value = emit_body(&mut func_ctx)?;
            func_ctx.builder.ins().return_(&[return_value]);
            func_ctx.builder.seal_all_blocks();
        }

        self.module
            .define_function(func_id, &mut cranelift_ctx)
            .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?;

        self.module.clear_context(&mut cranelift_ctx);

        Ok(func_id)
    }
}

pub struct ModuleContext<'a> {
    pub isa: Arc<dyn isa::TargetIsa>,
    pub module: &'a mut dyn Module,
    pub lambda_registry: &'a mut LambdaRegistry,
}

impl<'a> ModuleContext<'a> {
    fn new(
        isa: Arc<dyn isa::TargetIsa>,
        module: &'a mut JITModule,
        lambda_registry: &'a mut LambdaRegistry,
    ) -> Self {
        Self {
            isa,
            module,
            lambda_registry,
        }
    }
}

pub struct FunctionContext<'a, 'b> {
    pub builder: &'b mut FunctionBuilder<'a>,
    pub module: &'b mut dyn Module,
    pub lambda_registry: &'b mut LambdaRegistry,
    pub variables: HashMap<String, Variable>,
    pub blocks: HashMap<String, Block>,
    pub self_referencing_lambdas: HashMap<String, usize>, // name -> lambda_id
}

impl<'a, 'b> FunctionContext<'a, 'b> {
    fn new(
        builder: &'b mut FunctionBuilder<'a>,
        module: &'b mut dyn Module,
        lambda_registry: &'b mut LambdaRegistry,
    ) -> Self {
        Self {
            builder,
            module,
            lambda_registry,
            variables: HashMap::new(),
            blocks: HashMap::new(),
            self_referencing_lambdas: HashMap::new(),
        }
    }

    pub fn declare_variable(&mut self, name: impl Into<String>, ty: impl Into<Type>) -> Variable {
        let var = self.builder.declare_var(ty.into());
        self.variables.insert(name.into(), var);
        var
    }

    pub fn has_variable(&self, name: &str) -> Option<&Variable> {
        self.variables.get(name)
    }

    pub fn get_variable(&self, name: &str) -> Result<Variable> {
        self.variables
            .get(name)
            .copied()
            .ok_or_else(|| EmitError::UndefinedVariable.to_diagnostic())
    }

    pub fn create_block(&mut self, name: Option<String>) -> Block {
        let block = self.builder.create_block();
        if let Some(name) = name {
            self.blocks.insert(name, block);
        }
        block
    }

    pub fn emit<T: Emit>(&mut self, node: &T) -> Result<T::Output> {
        node.emit(self)
    }
}

pub struct LambdaRegistry {
    lambdas: HashMap<usize, (FuncId, Signature)>,
}

impl LambdaRegistry {
    pub fn new() -> Self {
        Self {
            lambdas: HashMap::new(),
        }
    }

    pub fn register(&mut self, lambda_id: usize, func_id: FuncId, signature: Signature) {
        self.lambdas.insert(lambda_id, (func_id, signature));
    }

    pub fn get(&self, lambda_id: usize) -> Option<&(FuncId, Signature)> {
        self.lambdas.get(&lambda_id)
    }
}
