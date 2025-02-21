use crate::{
    ast::{Statement, StatementValue, TypedExpression, TypedStatement},
    prelude::*,
};
use cranelift::{
    codegen::{
        control::ControlPlane,
        ir::{Function, UserFuncName},
        CompiledCode, Final,
    },
    prelude::{isa::CallConv, *},
};
use cranelift_module::Module;
use jit::Jit;
use std::path::PathBuf;

pub mod jit;

pub struct Compiler<'ast> {
    jit: Jit,
    expression: TypedExpression<'ast>,
}

impl<'ast> Compiler<'ast> {
    pub fn new(expression: TypedExpression<'ast>) -> Self {
        Self {
            jit: Jit::default(),
            expression,
        }
    }

    pub fn compile(&mut self) -> Result<CompiledCode> {
        let mut sig = Signature::new(CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I64));

        let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig);
        let mut func_builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut func_builder_ctx);

        let entry_block = builder.create_block();
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let const1 = builder.ins().iconst(types::I64, 1);
        let sum = builder.ins().iadd(const1, const1);
        builder.ins().return_(&[sum]);

        builder.finalize();

        println!("generated ir:\n{}", func.display());

        let isa_builder = cranelift_native::builder().unwrap();
        let flag_builder = cranelift::codegen::settings::builder();
        let flags = cranelift::codegen::settings::Flags::new(flag_builder);
        let isa = isa_builder.finish(flags).unwrap();

        let mut context = cranelift::codegen::Context::for_function(func);
        let mut ctrl_plane = ControlPlane::default();
        Ok(context.compile(&*isa, &mut ctrl_plane).unwrap().clone())
    }
}
