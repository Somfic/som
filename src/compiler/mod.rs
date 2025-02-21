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
use cranelift_module::{Linkage, Module};
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
        self.jit
            .ctx
            .func
            .signature
            .returns
            .push(AbiParam::new(types::I64));
        self.jit.ctx.func.name = UserFuncName::user(0, 0);

        {
            let mut builder =
                FunctionBuilder::new(&mut self.jit.ctx.func, &mut self.jit.builder_context);
            let entry_block = builder.create_block();
            builder.switch_to_block(entry_block);
            builder.seal_block(entry_block);

            match &self.expression.value {
                crate::ast::ExpressionValue::Primitive(primitive) => todo!(),
                crate::ast::ExpressionValue::Binary {
                    operator,
                    left,
                    right,
                } => match operator {
                    crate::ast::BinaryOperator::Add => {
                        let left = match &left.value {
                            crate::ast::ExpressionValue::Primitive(primitive) => match primitive {
                                crate::ast::Primitive::Integer(value) => {
                                    builder.ins().iconst(types::I64, *value)
                                }
                                _ => todo!(),
                            },
                            _ => todo!(),
                        };

                        let right = match &right.value {
                            crate::ast::ExpressionValue::Primitive(primitive) => match primitive {
                                crate::ast::Primitive::Integer(value) => {
                                    builder.ins().iconst(types::I64, *value)
                                }
                                _ => todo!(),
                            },
                            _ => todo!(),
                        };

                        let sum = builder.ins().iadd(left, right);
                        builder.ins().return_(&[sum]);
                    }
                    _ => todo!(),
                },
            }

            builder.finalize();
        }

        let mut ctrl_plane = ControlPlane::default();
        Ok(self
            .jit
            .ctx
            .compile(self.jit.module.isa(), &mut ctrl_plane)
            .expect("compilation failed")
            .clone())
    }
}
