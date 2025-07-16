use std::{collections::HashMap, sync::Arc};

use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;
pub use environment::Environment;

use crate::{
    compiler::environment::DeclarationValue,
    expressions,
    prelude::*,
    statements::{self},
};

pub mod environment;
pub mod external;

pub struct Compiler {
    pub isa: Arc<dyn isa::TargetIsa>,
    pub codebase: JITModule,
    declarations: HashMap<String, DeclarationValue>,
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

        let (codebase, declarations) = external::init_codebase();

        Self {
            isa,
            codebase,
            declarations,
        }
    }

    pub fn compile(&mut self, statement: &TypedStatement) -> *const u8 {
        let mut env = Environment::new(self.declarations.clone());

        let main_func_id = match &statement.value {
            StatementValue::VariableDeclaration(declaration) => match &declaration.value.value {
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
    ) {
        match &statement.value {
            StatementValue::Expression(expression) => {
                self.compile_expression(expression, body, env);
            }
            StatementValue::VariableDeclaration(_) => {
                statements::variable_declaration::compile(self, statement, body, env)
            }
            StatementValue::ExternDeclaration(_) => {
                statements::extern_declaration::compile(self, statement, body, env)
            }
            StatementValue::TypeDeclaration(_) => {
                statements::type_declaration::compile(self, statement, body, env)
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
                PrimaryExpression::I32(_) => {
                    expressions::primary::integer::compile_i32(self, expression, body, env)
                }
                PrimaryExpression::I64(_) => {
                    expressions::primary::integer::compile_i64(self, expression, body, env)
                }
                PrimaryExpression::Boolean(_) => {
                    expressions::primary::boolean::compile(self, expression, body, env)
                }
                PrimaryExpression::String(_) => {
                    expressions::primary::string::compile(self, expression, body, env)
                }
            },
            TypedExpressionValue::Unary(unary) => match &unary.operator {
                UnaryOperator::Negative => {
                    expressions::unary::negative::compile(self, expression, body, env)
                }
                op => todo!("Unary operator {:?} not implemented", op),
            },
            TypedExpressionValue::Binary(binary) => match binary.operator {
                BinaryOperator::Add => {
                    expressions::binary::add::compile(self, expression, body, env)
                }
                BinaryOperator::Subtract => {
                    expressions::binary::subtract::compile(self, expression, body, env)
                }
                BinaryOperator::Multiply => {
                    expressions::binary::multiply::compile(self, expression, body, env)
                }
                BinaryOperator::Divide => {
                    expressions::binary::divide::compile(self, expression, body, env)
                }
            },
            TypedExpressionValue::Identifier(_) => {
                expressions::identifier::compile(self, expression, body, env)
            }
            TypedExpressionValue::Block(block) => {
                expressions::block::compile(self, block, body, env)
            }
            TypedExpressionValue::Call(_) => {
                expressions::call::compile(self, expression, body, env)
            }
            TypedExpressionValue::Conditional(_) => {
                expressions::conditional::compile(self, expression, body, env)
            }
            TypedExpressionValue::StructConstructor(_) => {
                expressions::struct_constructor::compile(self, expression, body, env)
            }
            TypedExpressionValue::FieldAccess(_) => {
                expressions::field_access::compile(self, expression, body, env)
            }
            TypedExpressionValue::Assignment(_) => {
                expressions::assignment::compile(self, expression, body, env)
            }
            TypedExpressionValue::Group(_) => {
                expressions::group::compile(self, expression, body, env)
            }
            _ => todo!(
                "compilation for expression type {:?} not implemented",
                expression.value
            ),
        }
    }
}
