use std::sync::Arc;

use cranelift::{
    module::*,
    object::*,
    prelude::{settings::Flags, *},
};
use target_lexicon::Triple;

use crate::{Diagnostic, Type, TypedAst};

type Result<T> = std::result::Result<T, Diagnostic>;

enum CodegenError {
    NoMainFunction,
    NoReturnType,
    UnsupportedTarget(String),
    ModuleError(String),
}

impl CodegenError {
    fn to_diagnostic(self) -> Diagnostic {
        match self {
            CodegenError::NoMainFunction => {
                Diagnostic::error("no `main` function found".to_string())
                    .with_hint("add a `main` function as the entry point")
            }
            CodegenError::NoReturnType => {
                Diagnostic::error("`main` function must have a return type".to_string())
            }
            CodegenError::UnsupportedTarget(target) => {
                Diagnostic::error(format!("unsupported target: {}", target))
            }
            CodegenError::ModuleError(msg) => {
                Diagnostic::error(format!("code generation failed: {}", msg))
            }
        }
    }
}

pub struct Codegen<'ast> {
    isa: Arc<dyn isa::TargetIsa>,
    typed_ast: &'ast TypedAst,
    module: ObjectModule,
}

impl<'ast> Codegen<'ast> {
    pub fn new(typed_ast: &'ast TypedAst, triple: Triple) -> Result<Self> {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "true").unwrap();

        let triple_str = triple.to_string();
        let isa = isa::lookup(triple)
            .map_err(|_| CodegenError::UnsupportedTarget(triple_str).to_diagnostic())?
            .finish(Flags::new(flag_builder))
            .map_err(|e| CodegenError::ModuleError(e.to_string()).to_diagnostic())?;

        let builder = ObjectBuilder::new(isa.clone(), "main", default_libcall_names())
            .map_err(|e| CodegenError::ModuleError(e.to_string()).to_diagnostic())?;
        let module = ObjectModule::new(builder);

        Ok(Self {
            typed_ast,
            isa,
            module,
        })
    }

    pub fn compile(mut self) -> Result<ObjectProduct> {
        let main_function = self
            .typed_ast
            .ast
            .funcs
            .iter()
            .find(|f| f.name.value == "main".into())
            .ok_or_else(|| CodegenError::NoMainFunction.to_diagnostic())?;

        let return_type = main_function
            .return_type
            .as_ref()
            .ok_or_else(|| CodegenError::NoReturnType.to_diagnostic())?;

        let mut ctx = self.module.make_context();

        // function signature
        let mut sig = Signature::new(self.isa.default_call_conv());
        sig.params = main_function
            .parameters
            .iter()
            .filter_map(|p| p.ty.as_ref().map(|ty| AbiParam::new(to_type(ty))))
            .collect();
        sig.returns = vec![AbiParam::new(to_type(return_type))];
        ctx.func.signature = sig.clone();

        let mut func_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);

        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let return_value = builder.ins().iconst(types::I32, 42);
        builder.ins().return_(&[return_value]);

        builder.finalize();

        // Print Cranelift IR
        println!("{}", ctx.func);

        let func_id = self
            .module
            .declare_function("main", Linkage::Export, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()).to_diagnostic())?;
        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| CodegenError::ModuleError(e.to_string()).to_diagnostic())?;

        self.module.clear_context(&mut ctx);

        Ok(self.module.finish())
    }
}

pub fn to_type(ty: &Type) -> cranelift::prelude::Type {
    const POINTER: cranelift::prelude::Type = types::I64;

    match ty {
        Type::Unit => types::I8,
        Type::Unknown(..) => unreachable!("inferred during type inference"),
        Type::Named(..) => POINTER,
        Type::I32 => types::I32,
        Type::Bool => types::I8,
        Type::Str => POINTER,
        Type::Reference { .. } => POINTER,
        Type::Fun { .. } => POINTER,
    }
}
