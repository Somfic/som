use crate::{ast::Expression, EmitError, Result, Typed};
use cranelift::{
    codegen::ir::UserFuncName,
    jit::{JITBuilder, JITModule},
    module::{self, Linkage, Module},
    prelude::{
        settings::{builder, Flags},
        *,
    },
};
use std::{collections::HashMap, sync::Arc};
use target_lexicon::Triple;

mod expr;

pub trait Emit {
    type Output;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output>;
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
        // wrap the expression in the main function
        let mut sig = Signature::new(self.isa.default_call_conv());
        sig.params = vec![];
        sig.returns = vec![AbiParam::new(types::I64)];

        let main_function = self
            .module
            .declare_function("main", Linkage::Export, &sig)
            .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?;

        let mut ctx = self.module.make_context();
        ctx.func.signature = sig;
        ctx.func.name = UserFuncName::user(0, main_function.as_u32());

        let mut builder_context = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_context);

        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);

        {
            let mut emit_context = EmitContext::new(&mut builder, &mut self.module);
            let value = expression.emit(&mut emit_context)?;

            // cast to i64
            let value = emit_context.builder.ins().sextend(types::I64, value);

            emit_context.builder.ins().return_(&[value]);
            emit_context.builder.seal_all_blocks();

            drop(emit_context);
        };

        self.module
            .define_function(main_function, &mut ctx)
            .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?;

        self.module.clear_context(&mut ctx);

        self.module
            .finalize_definitions()
            .map_err(|err| EmitError::ModuleError(err).to_diagnostic())?;

        let main_function_code = self.module.get_finalized_function(main_function);

        Ok(main_function_code)
    }
}

pub struct EmitContext<'a> {
    pub builder: &'a mut FunctionBuilder<'a>,
    pub module: &'a mut dyn Module,
    pub variables: HashMap<String, Variable>,
    pub blocks: HashMap<String, Block>,
}

impl<'a> EmitContext<'a> {
    pub fn new(builder: &'a mut FunctionBuilder<'a>, module: &'a mut dyn Module) -> Self {
        Self {
            builder,
            module,
            variables: HashMap::new(),
            blocks: HashMap::new(),
        }
    }

    pub fn declare_variable(&mut self, name: String, ty: Type) -> Variable {
        let var = self.builder.declare_var(ty);
        self.variables.insert(name, var);
        var
    }

    pub fn get_variable(&self, name: &str) -> Result<Variable> {
        self.variables
            .get(name)
            .copied()
            .ok_or_else(|| EmitError::UndefinedVariable(name.to_string()).to_diagnostic())
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
