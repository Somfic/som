use crate::{
    ast::{
        BinaryOperator, ExpressionValue, Primitive, StatementValue, TypedExpression, TypedModule,
        TypedStatement, TypingValue,
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
    prelude::{
        isa::{CallConv, TargetIsa},
        *,
    },
};
use cranelift_module::{Linkage, Module};

use cranelift_jit::{JITBuilder, JITModule};
use environment::CompileEnvironment;

pub mod environment;

pub struct Compiler {}

impl Compiler {
    pub fn new() -> Self {
        Self {}
    }

    pub fn compile<'ast>(
        &mut self,
        modules: Vec<TypedModule<'ast>>,
    ) -> CompilerResult<CompiledCode> {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        let isa_builder = cranelift_native::builder().unwrap_or_else(|msg| {
            panic!("host machine is not supported: {}", msg);
        });
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();

        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

        let mut compiled_modules = vec![];

        for module in modules {
            compiled_modules.push(self.compile_module(&module));
        }

        todo!()
    }

    fn compile_module<'ast>(
        &mut self,
        builder: JITBuilder,
        module: &TypedModule<'ast>,
    ) -> JITModule {
        let mut jit_module = JITModule::new(builder);

        let environment = &mut CompileEnvironment::new();

        // declare functions
        for function in &module.functions {
            let mut signature = Signature::new(isa.default_call_conv());

            function.parameters.iter().for_each(|p| {
                signature
                    .params
                    .push(AbiParam::new(convert_type(&p.1.value)));
            });

            signature
                .returns
                .push(AbiParam::new(convert_type(&function.expression.ty.value)));

            environment.declare_function(function.name.clone(), signature, &mut jit_module);
        }

        // compile functions
        for function in &module.functions {
            let mut context = jit_module.make_context();
            let (func_id, signature) = environment.lookup_function(&function.name).unwrap();

            // TODO Get the function through the func_id
            context.func =
                Function::with_name_signature(UserFuncName::user(0, 0), signature.clone());

            let mut func_ctx = FunctionBuilderContext::new();
            let mut builder = FunctionBuilder::new(&mut context.func, &mut func_ctx);

            let entry_block = builder.create_block();
            builder.append_block_params_for_function_params(entry_block);
            builder.switch_to_block(entry_block);

            let body = compile_expression(&function.expression, &mut builder, environment);
            builder.ins().return_(&[body]);
            builder.finalize();
        }

        jit_module
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
            let var = environment.lookup_variable(name).unwrap().clone();
            builder.use_var(var)
        }
        _ => unimplemented!("{primitive:?}"),
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

pub(crate) fn convert_type(ty: &TypingValue) -> types::Type {
    match ty {
        TypingValue::Integer => types::I64,
        TypingValue::Boolean => types::I8,
        _ => panic!("unsupported type"),
    }
}
