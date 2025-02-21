use crate::{
    ast::{
        BinaryOperator, ExpressionValue, Primitive, Statement, StatementValue, TypedExpression,
        TypedStatement,
    },
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
            let builder_context = &mut self.jit.builder_context;
            let expression = &self.expression;

            let mut builder = FunctionBuilder::new(&mut self.jit.ctx.func, builder_context);
            let entry_block = builder.create_block();
            builder.switch_to_block(entry_block);
            builder.seal_block(entry_block);

            let value = Self::compile_expression(expression, &mut builder);
            builder.ins().return_(&[value]);

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

    fn compile_expression(
        expression: &TypedExpression<'ast>,
        builder: &mut FunctionBuilder,
    ) -> Value {
        match &expression.value {
            ExpressionValue::Primitive(p) => Self::compile_primitive(p, builder),
            ExpressionValue::Binary {
                operator,
                left,
                right,
            } => {
                let left_val = Self::compile_expression(left, builder);
                let right_val = Self::compile_expression(right, builder);
                match operator {
                    BinaryOperator::Add => builder.ins().iadd(left_val, right_val),
                    _ => unimplemented!(),
                }
            }
        }
    }

    fn compile_primitive(primitive: &Primitive<'ast>, builder: &mut FunctionBuilder) -> Value {
        match primitive {
            Primitive::Integer(v) => builder.ins().iconst(types::I64, *v),
            _ => unimplemented!(),
        }
    }
}
