use std::sync::Arc;

use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;
pub use environment::Environment;

use crate::{
    expressions,
    prelude::*,
    statements::{self},
};

pub mod environment;

pub struct Compiler {
    pub isa: Arc<dyn isa::TargetIsa>,
    pub codebase: JITModule,
}

impl Compiler {
    pub fn new() -> Self {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();

        let isa_builder = cranelift_native::builder().unwrap_or_else(|msg| {
            panic!("host machine is not supported: {msg}");
        });
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();

        let builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();
        let codebase = JITModule::new(builder);

        Self { isa, codebase }
    }

    pub fn compile(&mut self, statement: &TypedStatement) -> *const u8 {
        let mut env = Environment::new();

        let main_func_id = match &statement.value {
            StatementValue::Declaration(declaration) => match &declaration.value.value {
                TypedExpressionValue::Function(_) => {
                    expressions::function::compile(self, &declaration.value, &mut env)
                }
                _ => panic!("expected a function declaration"),
            },
            _ => panic!("expected a declaration statement"),
        };

        self.codebase.finalize_definitions().unwrap();
        self.codebase.get_finalized_function(main_func_id)
    }

    pub fn compile_statement(
        &mut self,
        statement: &TypedStatement,
        body: &mut FunctionBuilder,
        env: &mut Environment,
    ) -> CompileValue {
        match &statement.value {
            StatementValue::Expression(expression) => {
                self.compile_expression(expression, body, env)
            }
            StatementValue::Declaration(_) => {
                statements::declaration::compile(self, statement, body, env)
            }
        }
    }

    pub fn compile_expression(
        &mut self,
        expression: &TypedExpression,
        body: &mut FunctionBuilder,
        env: &mut CompileEnvironment,
    ) -> CompileValue {
        match &expression.value {
            TypedExpressionValue::Primary(primary) => match primary {
                PrimaryExpression::Unit => {
                    expressions::primary::unit::compile(self, expression, body, env)
                }
                PrimaryExpression::Integer(_) => {
                    expressions::primary::integer::compile(self, expression, body, env)
                }
                PrimaryExpression::Boolean(_) => {
                    expressions::primary::boolean::compile(self, expression, body, env)
                }
            },
            _ => todo!(),
        }
    }
}
