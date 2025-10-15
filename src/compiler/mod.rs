use std::{collections::HashMap, sync::Arc};

use cranelift::prelude::*;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Module};
pub use environment::Environment;

use crate::{
    compiler::environment::DeclarationValue,
    expressions,
    prelude::*,
    statements::{self},
};

pub mod capture;
pub mod environment;
pub mod external;

/// Context for tracking tail call positions during compilation
#[derive(Clone, Copy)]
pub enum TailContext {
    /// We're in tail position within the given function
    InTail { func_id: FuncId, loop_start: Block },
    /// We're not in tail position
    NotInTail,
}

pub struct Compiler {
    pub isa: Arc<dyn isa::TargetIsa>,
    pub codebase: JITModule,
    pub declarations: HashMap<String, DeclarationValue>,
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

    pub fn compile(&mut self, statement: &TypedStatement) -> Result<(*const u8, TypeValue)> {
        let mut env = Environment::new(self.declarations.clone());

        let (main_func_id, return_type) = match &statement.value {
            StatementValue::VariableDeclaration(declaration) => match &declaration.value.value {
                TypedExpressionValue::Function(func) => {
                    let return_type = func.body.type_.value.clone();
                    let (func_id, _captured_vars) =
                        expressions::function::compile(self, &declaration.value, &mut env);
                    (func_id, return_type)
                }
                _ => {
                    return Err(Error::Compiler(CompilerError::CodeGenerationFailed {
                        span: statement.span,
                        help: "Expected a function declaration".to_string(),
                    }));
                }
            },
            _ => {
                return Err(Error::Compiler(CompilerError::CodeGenerationFailed {
                    span: statement.span,
                    help: "Expected a declaration statement".to_string(),
                }));
            }
        };

        // Handle finalization errors
        self.codebase.finalize_definitions().map_err(|e| {
            Error::Compiler(CompilerError::FinalizationFailed {
                help: format!("Failed to finalize function: {}", e),
            })
        })?;

        Ok((
            self.codebase.get_finalized_function(main_func_id),
            return_type,
        ))
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
            StatementValue::Import(_) => statements::import::compile(self, statement, body, env),
        }
    }

    /// Compile an expression without tail call context (non-tail position)
    pub fn compile_expression(
        &mut self,
        expression: &TypedExpression,
        body: &mut FunctionBuilder,
        env: &mut CompileEnvironment,
    ) -> CompileValue {
        self.compile_expression_with_tail(expression, body, env, TailContext::NotInTail)
    }

    /// Compile an expression with tail call context
    pub fn compile_expression_with_tail(
        &mut self,
        expression: &TypedExpression,
        body: &mut FunctionBuilder,
        env: &mut CompileEnvironment,
        tail_ctx: TailContext,
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
                UnaryOperator::Negate => {
                    todo!()
                    // expressions::unary::negate::compile(self, expression, body, env)
                }
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
                BinaryOperator::LessThan => {
                    expressions::binary::less_than::compile(self, expression, body, env)
                }
                BinaryOperator::GreaterThan => {
                    expressions::binary::greater_than::compile(self, expression, body, env)
                }
                BinaryOperator::GreaterThanOrEqual => {
                    expressions::binary::greater_than_or_equal::compile(self, expression, body, env)
                }
                BinaryOperator::Equals => {
                    expressions::binary::equals::compile(self, expression, body, env)
                }
            },
            TypedExpressionValue::Identifier(_) => {
                expressions::identifier::compile(self, expression, body, env)
            }
            TypedExpressionValue::Block(block) => {
                expressions::block::compile(self, block, body, env, tail_ctx)
            }
            TypedExpressionValue::Call(_) => {
                expressions::call::compile(self, expression, body, env, tail_ctx)
            }
            TypedExpressionValue::Conditional(_) => {
                expressions::conditional::compile(self, expression, body, env, tail_ctx)
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
            TypedExpressionValue::Function(_) => {
                // Compile function expression and return a function pointer
                let (func_id, _captured_vars) =
                    expressions::function::compile(self, expression, env);

                // Get the function address as a value
                let func_ref = self.codebase.declare_func_in_func(func_id, body.func);
                let func_addr = body.ins().func_addr(self.isa.pointer_type(), func_ref);
                func_addr
            }
            TypedExpressionValue::WhileLoop(_) => {
                expressions::while_loop::compile(self, expression, body, env)
            }
        }
    }
}
