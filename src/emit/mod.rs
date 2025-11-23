use crate::{ast::Expression, EmitError, Result, Typed};
use cranelift::{
    codegen::ir::UserFuncName,
    module::{self, FuncId, Linkage, Module},
    object::{ObjectBuilder, ObjectModule},
    prelude::{settings::Flags, *},
};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};
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
    module_builder: ObjectBuilder,
}

impl Emitter {
    pub fn new(triple: Triple) -> Result<Self> {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "true").unwrap();

        let isa = isa::lookup(triple)
            .unwrap()
            .finish(Flags::new(flag_builder))
            .unwrap();

        let module_builder =
            ObjectBuilder::new(isa.clone(), "main", module::default_libcall_names())
                .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?;

        Ok(Self {
            isa,
            module_builder,
        })
    }

    fn new_module(&self, name: impl Into<String>) -> Result<ObjectModule> {
        Ok(ObjectModule::new(
            ObjectBuilder::new(
                self.isa.clone(),
                name.into(),
                module::default_libcall_names(),
            )
            .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?,
        ))
    }

    pub fn compile(&mut self, expression: &Expression<Typed>) -> Result<PathBuf> {
        let mut lambda_registry = LambdaRegistry::new();
        let mut extern_registry = HashMap::new();

        let mut module = self.new_module("main")?;

        let mut module_context = ModuleContext::new(
            self.isa.clone(),
            &mut module,
            &mut lambda_registry,
            &mut extern_registry,
        );

        expression.declare(&mut module_context)?;

        // wrap the expression in the main function
        let mut main_signature = Signature::new(self.isa.default_call_conv());
        main_signature.params = vec![];
        main_signature.returns = vec![AbiParam::new(types::I64)];

        let main_id = self.compile_function(
            "main",
            main_signature,
            Linkage::Export,
            &mut module_context,
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

        let bytes = module
            .finish()
            .emit()
            .map_err(|err| EmitError::WriteError(err).to_diagnostic());

        fs::create_dir_all("build").map_err(|err| {
            EmitError::IoError(err)
                .to_diagnostic()
                .with_hint("could not create build directory")
        })?;

        let output_path = PathBuf::from("build/main.o");
        fs::write(&output_path, bytes?).map_err(|err| {
            EmitError::IoError(err)
                .to_diagnostic()
                .with_hint("could not write output object file")
        })?;

        Ok(output_path)
    }

    fn compile_function<F>(
        &mut self,
        name: &str,
        sig: Signature,
        linkage: Linkage,
        module_context: &mut ModuleContext,
        emit_body: F,
    ) -> Result<FuncId>
    where
        F: FnOnce(&mut FunctionContext) -> Result<Value>,
    {
        let func_id = module_context
            .module
            .declare_function(name, linkage, &sig)
            .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?;

        let mut cranelift_ctx = module_context.module.make_context();
        cranelift_ctx.func.signature = sig;
        cranelift_ctx.func.name = UserFuncName::user(0, func_id.as_u32());

        let mut builder_context = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut cranelift_ctx.func, &mut builder_context);

        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);

        {
            let mut func_ctx = FunctionContext::new(
                &mut builder,
                module_context.module,
                module_context.lambda_registry,
                module_context.extern_registry,
            );
            let return_value = emit_body(&mut func_ctx)?;
            func_ctx.builder.ins().return_(&[return_value]);
            func_ctx.builder.seal_all_blocks();
        }

        module_context
            .module
            .define_function(func_id, &mut cranelift_ctx)
            .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?;

        module_context.module.clear_context(&mut cranelift_ctx);

        Ok(func_id)
    }
}

pub struct ModuleContext<'a> {
    pub isa: Arc<dyn isa::TargetIsa>,
    pub module: &'a mut dyn Module,
    pub lambda_registry: &'a mut LambdaRegistry,
    pub extern_registry: &'a mut HashMap<String, (FuncId, Signature)>,
}

impl<'a> ModuleContext<'a> {
    fn new(
        isa: Arc<dyn isa::TargetIsa>,
        module: &'a mut ObjectModule,
        lambda_registry: &'a mut LambdaRegistry,
        extern_registry: &'a mut HashMap<String, (FuncId, Signature)>,
    ) -> Self {
        Self {
            isa,
            module,
            lambda_registry,
            extern_registry,
        }
    }
}

pub struct FunctionContext<'a, 'b> {
    pub builder: &'b mut FunctionBuilder<'a>,
    pub module: &'b mut dyn Module,
    pub lambda_registry: &'b LambdaRegistry,
    pub extern_registry: &'b HashMap<String, (FuncId, Signature)>,
    pub variables: HashMap<String, Variable>,
    pub blocks: HashMap<String, Block>,
    pub self_referencing_lambdas: HashMap<String, usize>, // name -> lambda_id
}

impl<'a, 'b> FunctionContext<'a, 'b> {
    fn new(
        builder: &'b mut FunctionBuilder<'a>,
        module: &'b mut dyn Module,
        lambda_registry: &'b LambdaRegistry,
        extern_registry: &'b HashMap<String, (FuncId, Signature)>,
    ) -> Self {
        Self {
            builder,
            module,
            lambda_registry,
            extern_registry,
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
