use crate::{
    ast::{
        BinaryOperator, ExpressionValue, Module, Primitive, StatementValue, TypeValue,
        TypedExpression, TypedModule, TypedStatement,
    },
    prelude::*,
};
use cranelift::{
    codegen::{
        control::ControlPlane,
        ir::{function, Function, UserFuncName},
        verifier::VerifierError,
        CompileError, CompiledCode,
    },
    prelude::*,
};
use environment::CompileEnvironment;
use jit::Jit;
use std::env;

pub mod environment;
pub mod jit;

pub struct Compiler {
    jit: Jit,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            jit: Jit::default(),
        }
    }

    pub fn compile<'ast>(
        &mut self,
        modules: Vec<TypedModule<'ast>>,
    ) -> CompilerResult<CompiledCode> {
        let mut compiled_modules = vec![];

        for module in modules {
            compiled_modules.append(self.compile_module(&module)?);
        }

        println!("{}", self.jit.module.finalize_definitions());

        let mut ctrl_plane = ControlPlane::default();
        self.jit
            .ctx
            .compile(self.jit.module.isa(), &mut ctrl_plane)
            .map_err(parse_error)
            .cloned()
    }

    fn compile_module<'ast>(&mut self, module: &TypedModule<'ast>) -> Result<CompiledCode> {
        let environment = &mut CompileEnvironment::new();

        for (function_name, function) in &module.functions {
            let mut builder = environment.declare_function(self, function_name.clone());
            let body = compile_expression(&function.expression, &mut builder, environment);

            builder.ins().return_(&[body]);
            builder.finalize();
        }

        let mut ctrl_plane = ControlPlane::default();
        self.jit
            .ctx
            .compile(self.jit.module.isa(), &mut ctrl_plane)
            .map_err(parse_error)
            .cloned()
    }
}

fn compile_expression<'ast>(
    expression: &TypedExpression<'ast>,
    builder: &mut FunctionBuilder,
    environment: &mut CompileEnvironment<'ast>,
) -> Value {
    match &expression.value {
        ExpressionValue::Primitive(p) => compile_primitive(p, builder, environment),
        ExpressionValue::Binary {
            operator,
            left,
            right,
        } => {
            let left_val = compile_expression(left, builder, environment);
            let right_val = compile_expression(right, builder, environment);
            match operator {
                BinaryOperator::Add => builder.ins().iadd(left_val, right_val),
                BinaryOperator::Subtract => builder.ins().isub(left_val, right_val),
                BinaryOperator::Multiply => builder.ins().imul(left_val, right_val),
                BinaryOperator::Divide => builder.ins().udiv(left_val, right_val),
                _ => unimplemented!("{operator:?}"),
            }
        }
        ExpressionValue::Group(expression) => compile_expression(expression, builder, environment),
        ExpressionValue::Unary { operator, operand } => match operator {
            crate::ast::UnaryOperator::Negate => todo!(),
            crate::ast::UnaryOperator::Negative => {
                let value = compile_expression(operand, builder, environment);
                builder.ins().ineg(value)
            }
        },
        ExpressionValue::Conditional {
            condition,
            truthy,
            falsy,
        } => {
            let condition = compile_expression(condition, builder, environment);
            let truthy = compile_expression(truthy, builder, environment);
            let falsy = compile_expression(falsy, builder, environment);

            // TODO: Check if this is fine? The resulting IR runs both branches and then
            //  selects the result to return, but really we should only run the branch that
            //  is selected based on the condition.
            builder.ins().select(condition, truthy, falsy)
        }
        ExpressionValue::Block { statements, result } => {
            // open a new block
            let block = builder.create_block();
            builder.append_block_param(block, convert_type(&result.ty.value));

            for statement in statements {
                compile_statement(statement, builder, environment);
            }

            let result = compile_expression(result, builder, environment);

            builder.ins().jump(block, &[result]);
            builder.switch_to_block(block);
            builder.seal_block(block);

            builder.block_params(block)[0]
        }
        _ => unimplemented!("{expression:?}"),
    }
}

fn compile_statement<'ast>(
    statement: &TypedStatement<'ast>,
    builder: &mut FunctionBuilder,
    environment: &mut CompileEnvironment<'ast>,
) {
    match &statement.value {
        StatementValue::Expression(expression) => {
            compile_expression(expression, builder, environment);
        }
        StatementValue::Block(statements) => {
            for statement in statements {
                compile_statement(statement, builder, environment);
            }
        }
        StatementValue::Declaration(name, expression) => {
            let value = compile_expression(expression, builder, environment);
            let var = environment.declare_variable(name.clone(), builder, &expression.ty.value);
            builder.def_var(var, value);
        }
        _ => unimplemented!("{statement:?}"),
    }
}

fn compile_primitive<'ast>(
    primitive: &Primitive<'ast>,
    builder: &mut FunctionBuilder,
    environment: &mut CompileEnvironment<'ast>,
) -> Value {
    match primitive {
        Primitive::Integer(v) => builder.ins().iconst(types::I64, *v),
        Primitive::Decimal(v) => builder.ins().f64const(*v),
        Primitive::Boolean(v) => builder.ins().iconst(types::I8, *v as i64),
        Primitive::Unit => builder.ins().iconst(types::I8, 0), // TODO: ideally we don't introduce a IR step for nothing ...
        Primitive::Identifier(name) => {
            // TODO: Convert between variable name and variable id
            let var = environment.lookup(name).unwrap().clone();
            builder.use_var(var)
        }
        _ => unimplemented!("{primitive:?}"),
    }
}

pub(crate) fn convert_type(ty: &TypeValue) -> types::Type {
    match ty {
        TypeValue::Integer => types::I64,
        TypeValue::Decimal => types::F64,
        TypeValue::Boolean => types::I8,
        _ => panic!("unsupported type"),
    }
}

fn parse_error(error: CompileError<'_>) -> Vec<miette::Report> {
    match error.inner {
        codegen::CodegenError::Verifier(verifier_errors) => {
            verifier_errors
                .0
                .into_iter()
                .map(|e| miette::miette! {
                    help = "this is a bug in the generated cranelift IR",
                    labels = vec![e.label(error.func)],
                    "{0}", e
                 }.with_source_code(format!("{}", error.func))) 
                .collect()
        }
        codegen::CodegenError::ImplLimitExceeded => {
            vec![miette::miette!("implementation limit was exceeded")]
        }
        codegen::CodegenError::CodeTooLarge => {
            vec![miette::miette!("the compiled code is too large")]
        }
        codegen::CodegenError::Unsupported(feature) => {
            vec![miette::miette! {
                help = format!("the `{feature}` might have to be explicitly enabled"), 
                "unsupported feature: {feature}"}
            ]
        }
        codegen::CodegenError::RegisterMappingError(register_mapping_error) => vec![
            miette::miette!("failure to map cranelift register representation to a dwarf register representation: {register_mapping_error}"),
        ],
        codegen::CodegenError::Regalloc(checker_errors) => vec![
            miette::miette!("regalloc validation errors: {checker_errors:?}"),
        ],
        codegen::CodegenError::Pcc(pcc_error) => vec![
            miette::miette!("proof-carrying-code validation error: {pcc_error:?}"),
        ],
    }
}

trait Label {
    fn label(&self, func: &Function) -> miette::LabeledSpan;
}

impl Label for VerifierError {
    fn label(&self, func: &Function) -> miette::LabeledSpan {
        let func_display = format!("{func}");

        let (offset, length) = match &self.context {
            Some(snippet) => match func_display.find(snippet) {
                Some(offset) => (offset, snippet.chars().count()),
                None => (0, func_display.chars().count()),
            },
            None => (0, func_display.chars().count()),
        };

        miette::LabeledSpan::new(Some(self.message.clone()), offset, length)
    }
}
