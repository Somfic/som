use crate::{
    ast::{Statement, StatementValue, TypedExpression, TypedStatement},
    prelude::*,
};
use cranelift::prelude::*;
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

    pub fn compile(&mut self) -> Result<PathBuf> {
        todo!()

        // let function_name = "main";
        // let params = vec![];
        // let returns = vec![];
        // let statements = vec![TypedStatement {
        //     value: StatementValue::Expression(self.expression.clone()),
        //     span: self.expression.span,
        // }];

        // for parameter in &params {
        //     self.jit.ctx.func.signature.params.push(AbiParam::new(
        //         self.jit.module.target_config().pointer_type(),
        //     ));
        // }

        // for return_type in &returns {
        //     self.jit.ctx.func.signature.returns.push(AbiParam::new(
        //         self.jit.module.target_config().pointer_type(),
        //     ));
        // }

        // let mut builder =
        //     FunctionBuilder::new(&mut self.jit.ctx.func, &mut self.jit.builder_context);

        // let entry_block = builder.create_block();
        // builder.append_block_params_for_function_params(entry_block);

        // builder.switch_to_block(entry_block);

        // builder.seal_block(entry_block);

        // let variables =
        //     declare_variables(int, &mut builder, &params, &the_return, &stmts, entry_block);

        // Ok(PathBuf::new())
    }
}
