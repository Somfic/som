use crate::{
    ast::{
        BinaryOperator, ExpressionValue, Primitive, StatementValue, TypedExpression, TypedModule,
        TypedStatement, TypingValue,
    },
    prelude::*,
};
use cranelift::{
    codegen::{
        ir::{Function, UserFuncName},
        verifier::VerifierError,
        CompileError,
    },
    prelude::*,
};
use cranelift_module::Module;

use cranelift_jit::{JITBuilder, JITModule};
use environment::CompileEnvironment;

pub mod environment;

pub struct Compiler {}

impl Compiler {
    pub fn new() -> Self {
        Self {}
    }

    pub fn compile(&mut self, modules: Vec<TypedModule<'_>>) -> CompilerResult<*const u8> {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        let isa_builder = cranelift_native::builder().unwrap_or_else(|msg| {
            panic!("host machine is not supported: {}", msg);
        });
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();

        let builder = JITBuilder::new(cranelift_module::default_libcall_names()).unwrap();
        let mut jit_module = JITModule::new(builder);
        let mut environment = CompileEnvironment::new();

        // first, declare all functions from all modules
        for module in &modules {
            for function in &module.functions {
                let mut signature = Signature::new(isa.default_call_conv());
                for p in &function.parameters {
                    signature
                        .params
                        .push(AbiParam::new(convert_type(&p.1.value)));
                }
                signature
                    .returns
                    .push(AbiParam::new(convert_type(&function.body.ty.value)));
                environment.declare_function(function, signature, &mut jit_module);
            }
        }

        // next, compile each function
        for module in &modules {
            for function in &module.functions {
                let mut context = jit_module.make_context();
                let (func_id, signature, declaration) = {
                    let lookup = environment.lookup_function(&function.name).unwrap();
                    (lookup.0, lookup.1.clone(), lookup.2)
                };
                context.func = Function::with_name_signature(UserFuncName::user(0, 0), signature);
                let mut func_ctx = FunctionBuilderContext::new();
                let mut builder = FunctionBuilder::new(&mut context.func, &mut func_ctx);

                let entry_block = builder.create_block();
                builder.append_block_params_for_function_params(entry_block);
                builder.switch_to_block(entry_block);

                let block_params = builder.block_params(entry_block).to_vec();
                for (i, (param, param_ty)) in function.parameters.iter().enumerate() {
                    let variable =
                        environment.declare_variable(param.clone(), &mut builder, &param_ty.value);

                    builder.def_var(variable, block_params[i]);
                }

                // add function parameters to the environment

                let body = compile_expression(
                    &function.body,
                    &mut builder,
                    &mut environment.block(),
                    &mut jit_module,
                );
                builder.ins().return_(&[body]);
                builder.seal_block(entry_block);
                builder.finalize();

                // define the function in the jit_module
                jit_module.define_function(func_id, &mut context).unwrap();
            }
        }

        // finalize the entire module: this writes all relocations and finalizes code memory
        jit_module.finalize_definitions().unwrap();

        Ok(jit_module.get_finalized_function(environment.lookup_function("main").unwrap().0))
    }
}

fn compile_expression<'ast>(
    expression: &TypedExpression<'ast>,
    builder: &mut FunctionBuilder,
    environment: &mut CompileEnvironment<'ast>,
    jit_module: &mut JITModule,
) -> Value {
    match &expression.value {
        ExpressionValue::Primitive(p) => compile_primitive(p, builder, environment, jit_module),
        ExpressionValue::Binary {
            operator,
            left,
            right,
        } => {
            let left_val = compile_expression(left, builder, environment, jit_module);
            let right_val = compile_expression(right, builder, environment, jit_module);
            match operator {
                BinaryOperator::Add => builder.ins().iadd(left_val, right_val),
                BinaryOperator::Subtract => builder.ins().isub(left_val, right_val),
                BinaryOperator::Multiply => builder.ins().imul(left_val, right_val),
                BinaryOperator::Divide => builder.ins().udiv(left_val, right_val),
                BinaryOperator::LessThan => {
                    builder
                        .ins()
                        .icmp(IntCC::SignedLessThan, left_val, right_val)
                }
                _ => unimplemented!("{operator:?}"),
            }
        }
        ExpressionValue::Group(expression) => {
            compile_expression(expression, builder, environment, jit_module)
        }
        ExpressionValue::Unary { operator, operand } => match operator {
            crate::ast::UnaryOperator::Negate => todo!(),
            crate::ast::UnaryOperator::Negative => {
                let value = compile_expression(operand, builder, environment, jit_module);
                builder.ins().ineg(value)
            }
        },
        ExpressionValue::Conditional {
            condition,
            truthy,
            falsy,
        } => {
            let cond_val = compile_expression(condition, builder, environment, jit_module);
            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.append_block_param(merge_block, convert_type(&truthy.ty.value));

            builder
                .ins()
                .brif(cond_val, then_block, &[], else_block, &[]);

            // compile truthy branch
            builder.switch_to_block(then_block);
            let true_val = compile_expression(truthy, builder, environment, jit_module);
            builder.ins().jump(merge_block, &[true_val]);
            builder.seal_block(then_block);

            // compile falsy branch
            builder.switch_to_block(else_block);
            let false_val = compile_expression(falsy, builder, environment, jit_module);
            builder.ins().jump(merge_block, &[false_val]);
            builder.seal_block(else_block);

            // merge the branches
            builder.switch_to_block(merge_block);
            builder.seal_block(merge_block);
            builder.block_params(merge_block)[0]
        }
        ExpressionValue::Block { statements, result } => {
            // open a new block
            let block = builder.create_block();
            builder.append_block_param(block, convert_type(&result.ty.value));

            for statement in statements {
                compile_statement(statement, builder, environment, jit_module);
            }

            let result = compile_expression(result, builder, environment, jit_module);

            builder.ins().jump(block, &[result]);
            builder.switch_to_block(block);
            builder.seal_block(block);

            builder.block_params(block)[0]
        }
        ExpressionValue::FunctionCall {
            function_name,
            arguments,
        } => {
            let arg_values: Vec<Value> = arguments
                .iter()
                .map(|arg| compile_expression(arg, builder, environment, jit_module))
                .collect();

            let (func_id, _signature, _declaration) = environment
                .lookup_function(function_name)
                .expect("function not declared");

            let callee = jit_module.declare_func_in_func(*func_id, builder.func);
            let call_inst = builder.ins().call(callee, &arg_values);

            builder.inst_results(call_inst)[0]
        }
        ExpressionValue::Assignment { name, value } => {
            let value = compile_expression(value, builder, environment, jit_module);
            let var = environment.lookup_variable(name).unwrap();
            builder.def_var(*var, value);
            value
        }
    }
}

fn compile_statement<'ast>(
    statement: &TypedStatement<'ast>,
    builder: &mut FunctionBuilder,
    environment: &mut CompileEnvironment<'ast>,
    jit_module: &mut JITModule,
) {
    match &statement.value {
        StatementValue::Expression(expression) => {
            compile_expression(expression, builder, environment, jit_module);
        }
        StatementValue::Block(statements) => {
            for statement in statements {
                compile_statement(statement, builder, environment, jit_module);
            }
        }
        StatementValue::Declaration(name, expression) => {
            let value = compile_expression(expression, builder, environment, jit_module);
            let var = environment.declare_variable(name.clone(), builder, &expression.ty.value);
            builder.def_var(var, value);
        }
        StatementValue::Condition(condition, statement) => {
            let cond_val = compile_expression(condition, builder, environment, jit_module);
            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();

            builder
                .ins()
                .brif(cond_val, then_block, &[], else_block, &[]);

            // compile truthy branch
            builder.switch_to_block(then_block);
            let mut environment = environment.block();
            compile_statement(statement, builder, &mut environment, jit_module);
            builder.ins().jump(merge_block, &[]);
            builder.seal_block(then_block);

            // compile falsy branch
            builder.switch_to_block(else_block);
            builder.ins().jump(merge_block, &[]);
            builder.seal_block(else_block);

            // merge the branches
            builder.switch_to_block(merge_block);
            builder.seal_block(merge_block);
        }
        StatementValue::WhileLoop(condition, body) => {
            let cond_block = builder.create_block();
            let body_block = builder.create_block();
            let merge_block = builder.create_block();

            builder.ins().jump(cond_block, &[]);

            // compile condition block
            builder.switch_to_block(cond_block);
            let cond_val = compile_expression(condition, builder, environment, jit_module);
            builder
                .ins()
                .brif(cond_val, body_block, &[], merge_block, &[]);

            // compile body block
            builder.switch_to_block(body_block);
            compile_statement(body, builder, environment, jit_module);
            builder.ins().jump(cond_block, &[]);
            builder.seal_block(body_block);

            // merge block
            builder.switch_to_block(merge_block);
            builder.seal_block(merge_block);
            builder.seal_block(cond_block);
        }
    }
}

fn compile_primitive<'ast>(
    primitive: &Primitive<'ast>,
    builder: &mut FunctionBuilder,
    environment: &mut CompileEnvironment<'ast>,
    jit_module: &mut JITModule,
) -> Value {
    match primitive {
        Primitive::Integer(v) => builder.ins().iconst(types::I64, *v),
        Primitive::Decimal(v) => builder.ins().f64const(*v),
        Primitive::Boolean(v) => builder.ins().iconst(types::I8, *v as i64),
        Primitive::Unit => builder.ins().iconst(types::I8, 0), // TODO: ideally we don't introduce a IR step for nothing ...
        Primitive::Identifier(name) => {
            // TODO: Convert between variable name and variable id
            let var = *environment
                .lookup_variable(name)
                .unwrap_or_else(|| panic!("variable {name} not found"));
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
        TypingValue::Decimal => types::F64,
        TypingValue::Unknown => unreachable!("unknown type"),
        TypingValue::Symbol(cow) => unreachable!("{cow}"),
        TypingValue::Unit => types::I8,
    }
}
