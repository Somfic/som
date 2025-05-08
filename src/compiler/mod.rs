use std::sync::Arc;

use crate::{
    ast::{
        BinaryOperator, ExpressionValue, Identifier, IntrinsicSignature, FunctionSignature,
        Primitive, StatementValue, TypedExpression, TypedModule, TypedStatement, TypingValue,
    },
    prelude::*,
    typer::TyperResult,
};
use cranelift::{
    codegen::{
        ir::{Function, UserFuncName},
        verifier::VerifierError,
        CompileError,
    },
    prelude::{isa::TargetIsa, *},
};
use cranelift_module::Module;

use cranelift_jit::{JITBuilder, JITModule};
use environment::CompileEnvironment;

pub mod environment;

pub struct Compiler {
    isa: Arc<dyn TargetIsa>,
    codebase: JITModule,
    typed: TyperResult,
}

impl Compiler {
    pub fn new(typed: TyperResult) -> Self {
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
        let codebase = JITModule::new(builder);

        Self {
            isa,
            codebase,
            typed,
        }
    }

    pub fn compile(mut self) -> ReportResult<*const u8> {
        let mut environment = CompileEnvironment::new();

        let typed_modules = self.typed.modules.clone();

        for module in &typed_modules {
            self.declare_module(module, &mut environment);
        }

        for module in &typed_modules {
            self.compile_module(module, &mut environment);
        }

        self.codebase.finalize_definitions().unwrap();

        Ok(self.codebase.get_finalized_function(
            environment
                .lookup_function(&Identifier::new("main"))
                .unwrap()
                .0,
        ))
    }

    fn declare_module(&mut self, module: &TypedModule, environment: &mut CompileEnvironment) {
        for statement in &module.statements {
            match &statement.value {
                StatementValue::Declaration(identifier, _, value) => match &value.value {
                    ExpressionValue::Lambda {
                        parameters,
                        explicit_return_type,
                        body,
                    } => {
                        let signature = FunctionSignature {
                            span: statement.span,
                            parameters: parameters.clone(),
                            explicit_return_type: explicit_return_type.clone(),
                        };
                        self.declare_function(identifier.clone(), &signature, body, environment);
                    }
                    _ => todo!("non-function type {:?}", value.value),
                },
                _ => todo!("non-declaration statement in module {:?}", statement),
            }
        }
    }

    fn compile_module(&mut self, module: &TypedModule, environment: &mut CompileEnvironment) {
        for statement in &module.statements {
            match &statement.value {
                StatementValue::Declaration(identifier, _, value) => match &value.value {
                    ExpressionValue::Lambda {
                        parameters,
                        explicit_return_type,
                        body,
                    } => {
                        let signature = FunctionSignature {
                            span: statement.span,
                            parameters: parameters.clone(),
                            explicit_return_type: explicit_return_type.clone(),
                        };
                        self.compile_function(identifier.clone(), &signature, body, environment);
                    }
                    _ => todo!("non-function type {:?}", value.value),
                },
                _ => todo!("non-declaration statement in module {:?}", statement),
            }
        }
    }

    fn compile_expression(
        &mut self,
        expression: &TypedExpression,
        builder: &mut FunctionBuilder,
        environment: &mut CompileEnvironment,
    ) -> Value {
        match &expression.value {
            ExpressionValue::Primitive(p) => self.compile_primitive(p, builder, environment),
            ExpressionValue::Binary {
                operator,
                left,
                right,
            } => {
                let left_val = self.compile_expression(left, builder, environment);
                let right_val = self.compile_expression(right, builder, environment);
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
                    BinaryOperator::Modulo => todo!(),
                    BinaryOperator::Equality => {
                        builder.ins().icmp(IntCC::Equal, left_val, right_val)
                    }
                    BinaryOperator::Inequality => {
                        builder.ins().icmp(IntCC::NotEqual, left_val, right_val)
                    }
                    BinaryOperator::LessThanOrEqual => {
                        builder
                            .ins()
                            .icmp(IntCC::SignedLessThanOrEqual, left_val, right_val)
                    }
                    BinaryOperator::GreaterThan => {
                        builder
                            .ins()
                            .icmp(IntCC::SignedGreaterThan, left_val, right_val)
                    }
                    BinaryOperator::GreaterThanOrEqual => {
                        builder
                            .ins()
                            .icmp(IntCC::SignedGreaterThanOrEqual, left_val, right_val)
                    }
                    BinaryOperator::And => todo!(),
                    BinaryOperator::Or => todo!(),
                }
            }
            ExpressionValue::Group(expression) => {
                self.compile_expression(expression, builder, environment)
            }
            ExpressionValue::Unary { operator, operand } => match operator {
                crate::ast::UnaryOperator::Negate => {
                    let value = self.compile_expression(operand, builder, environment);
                    let scale = builder.ins().iconst(types::I8, -1);
                    builder.ins().iadd(value, scale)
                }
                crate::ast::UnaryOperator::Negative => {
                    let value = self.compile_expression(operand, builder, environment);
                    builder.ins().ineg(value)
                }
            },
            ExpressionValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let cond_val = self.compile_expression(condition, builder, environment);
                let then_block = builder.create_block();
                let else_block = builder.create_block();
                let merge_block = builder.create_block();

                builder.append_block_param(merge_block, truthy.ty.value.to_ir());

                builder
                    .ins()
                    .brif(cond_val, then_block, &[], else_block, &[]);

                // compile truthy branch
                builder.switch_to_block(then_block);
                let true_val = self.compile_expression(truthy, builder, environment);
                builder.ins().jump(merge_block, &[true_val]);
                builder.seal_block(then_block);

                // compile falsy branch
                builder.switch_to_block(else_block);
                let false_val = self.compile_expression(falsy, builder, environment);
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
                builder.append_block_param(block, result.ty.value.to_ir());

                for statement in statements {
                    self.compile_statement(statement, builder, environment);
                }

                let result = self.compile_expression(result, builder, environment);

                builder.ins().jump(block, &[result]);
                builder.switch_to_block(block);
                builder.seal_block(block);

                builder.block_params(block)[0]
            }
            ExpressionValue::FunctionCall {
                identifier: function_name,
                arguments,
            } => {
                let arg_values: Vec<Value> = arguments
                    .iter()
                    .map(|arg| self.compile_expression(arg, builder, environment))
                    .collect();

                let (func_id, _signature) = environment
                    .lookup_function(function_name)
                    .expect("function not declared");

                let callee = self.codebase.declare_func_in_func(*func_id, builder.func);
                let call_inst = builder.ins().call(callee, &arg_values);

                builder.inst_results(call_inst)[0]
            }
            ExpressionValue::VariableAssignment {
                identifier: name,
                argument: value,
            } => {
                let value = self.compile_expression(value, builder, environment);
                let var = environment.lookup_variable(name).unwrap();
                builder.def_var(*var, value);
                value
            }
            ExpressionValue::StructConstructor {
                identifier: _,
                arguments: _,
            } => todo!(),
            ExpressionValue::FieldAccess {
                parent_identifier: _,
                identifier: _,
            } => todo!(),
            ExpressionValue::Lambda {
                parameters,
                explicit_return_type,
                body,
            } => todo!(),
        }
    }

    fn compile_statement(
        &mut self,
        statement: &TypedStatement,
        builder: &mut FunctionBuilder,
        environment: &mut CompileEnvironment,
    ) {
        match &statement.value {
            StatementValue::Expression(expression) => {
                self.compile_expression(expression, builder, environment);
            }
            StatementValue::Block(statements) => {
                let mut environment = environment.block();
                for statement in statements {
                    self.compile_statement(statement, builder, &mut environment);
                }
            }
            StatementValue::Declaration(identifier, _, value) => {
                let var =
                    environment.declare_variable(identifier.clone(), builder, &value.ty.value);
                let value = self.compile_expression(value, builder, environment);
                builder.def_var(var, value);
            }
            StatementValue::Condition(condition, statement) => {
                let cond_val = self.compile_expression(condition, builder, environment);
                let then_block = builder.create_block();
                let else_block = builder.create_block();
                let merge_block = builder.create_block();

                builder
                    .ins()
                    .brif(cond_val, then_block, &[], else_block, &[]);

                // compile truthy branch
                builder.switch_to_block(then_block);
                let mut environment = environment.block();
                self.compile_statement(statement, builder, &mut environment);
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
                let cond_val = self.compile_expression(condition, builder, environment);
                builder
                    .ins()
                    .brif(cond_val, body_block, &[], merge_block, &[]);

                // compile body block
                builder.switch_to_block(body_block);
                self.compile_statement(body, builder, environment);
                builder.ins().jump(cond_block, &[]);
                builder.seal_block(body_block);

                // merge block
                builder.switch_to_block(merge_block);
                builder.seal_block(merge_block);
                builder.seal_block(cond_block);
            }
            StatementValue::TypeDeclaration(identifier, typing) => todo!(),
        }
    }

    fn declare_intrinsic_function(
        &mut self,
        identifier: Identifier,
        intrinsic_signature: &IntrinsicSignature,
        environment: &mut CompileEnvironment,
    ) {
        let mut signature = Signature::new(self.isa.default_call_conv());

        for parameter in &intrinsic_signature.parameters {
            let parameter_type = parameter.ty.value.to_ir();
            signature.params.push(AbiParam::new(parameter_type));
        }

        signature
            .returns
            .push(AbiParam::new(intrinsic_signature.return_type.value.to_ir()));

        environment.declare_function(identifier, signature, &mut self.codebase);
    }

    fn declare_function(
        &mut self,
        identifier: Identifier,
        function_signature: &FunctionSignature,
        body: &TypedExpression,
        environment: &mut CompileEnvironment,
    ) {
        let mut signature = Signature::new(self.isa.default_call_conv());

        for parameter in &function_signature.parameters {
            signature
                .params
                .push(AbiParam::new(parameter.ty.value.to_ir()));
        }

        signature.returns.push(AbiParam::new(body.ty.value.to_ir()));

        environment.declare_function(identifier, signature, &mut self.codebase);
    }

    fn compile_function(
        &mut self,
        identifier: Identifier,
        function_signature: &FunctionSignature,
        body: &TypedExpression,
        environment: &mut CompileEnvironment,
    ) {
        let mut context = self.codebase.make_context();
        let (func_id, signature) = {
            let lookup = environment.lookup_function(&identifier).unwrap();
            (lookup.0, lookup.1.clone())
        };
        context.func = Function::with_name_signature(UserFuncName::user(0, 0), signature);
        let mut func_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut context.func, &mut func_ctx);

        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);

        let block_params = builder.block_params(entry_block).to_vec();
        for (i, parameter) in function_signature.parameters.iter().enumerate() {
            let variable = environment.declare_variable(
                parameter.identifier.clone(),
                &mut builder,
                &parameter.ty.value,
            );

            builder.def_var(variable, block_params[i]);
        }

        let body = self.compile_expression(&body, &mut builder, environment);
        builder.ins().return_(&[body]);
        builder.seal_block(entry_block);
        builder.finalize();

        self.codebase
            .define_function(func_id, &mut context)
            .unwrap();
    }

    fn compile_intrinsic_function(
        &mut self,
        identifier: Identifier,
        intrinsic_signature: &IntrinsicSignature,
        environment: &mut CompileEnvironment,
    ) {
        let mut context = self.codebase.make_context();
        let (func_id, signature) = {
            let lookup = environment
                .lookup_function(&identifier)
                .unwrap_or_else(|| panic!("{} intrinsic function not found", identifier));
            (lookup.0, lookup.1.clone())
        };
        context.func = Function::with_name_signature(UserFuncName::user(0, 0), signature);
        let mut func_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut context.func, &mut func_ctx);

        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);

        let block_params = builder.block_params(entry_block).to_vec();
        for (i, parameter) in intrinsic_signature.parameters.iter().enumerate() {
            let variable = environment.declare_variable(
                parameter.identifier.clone(),
                &mut builder,
                &parameter.ty.value,
            );

            builder.def_var(variable, block_params[i]);
        }

        match &identifier.name[..] {
            "assert" => {
                let cond = block_params[0];
                let code = TrapCode::unwrap_user(123);
                builder.ins().trapz(cond, code);
                let unit_val = builder.ins().iconst(types::I8, 0);
                builder.ins().return_(&[unit_val]);
            }
            _ => panic!("unknown intrinsic function"),
        };

        builder.seal_block(entry_block);
        builder.finalize();

        self.codebase
            .define_function(func_id, &mut context)
            .unwrap();
    }

    fn compile_primitive(
        &mut self,
        primitive: &Primitive,
        builder: &mut FunctionBuilder,
        environment: &mut CompileEnvironment,
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

    fn parse_error(&mut self, error: CompileError<'_>) -> Vec<miette::Report> {
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

impl TypingValue {
    pub fn to_ir(&self) -> types::Type {
        match self {
            TypingValue::Integer => types::I64,
            TypingValue::Boolean => types::I8,
            TypingValue::Decimal => types::F64,
            TypingValue::Unknown => unreachable!("unknown type"),
            TypingValue::Symbol(identifier) => todo!(),
            TypingValue::Unit => types::I8,
            TypingValue::Generic(identifier) => todo!(),
            TypingValue::Struct(_) => todo!(),
            TypingValue::Function(lambda_signature) => todo!(),
        }
    }

    pub fn size_of(&self, environment: &CompileEnvironment) -> u32 {
        let ty = self.unzip_compile(environment);

        match ty {
            TypingValue::Unknown => 0,
            TypingValue::Integer => 4,
            TypingValue::Boolean => 1,
            TypingValue::Decimal => 4,
            TypingValue::Unit => 1,
            TypingValue::Symbol(_) => unreachable!(),
            TypingValue::Generic(_) => todo!(),
            TypingValue::Struct(members) => members
                .iter()
                .map(|m| m.ty.value.size_of(environment))
                .sum(),
            TypingValue::Function(lambda_signature) => todo!(),
        }
    }

    pub fn unzip_compile<'a>(&'a self, environment: &'a CompileEnvironment) -> &'a TypingValue {
        let unwrapped_ty = match &self {
            TypingValue::Symbol(identifier) => environment.lookup_type(identifier),
            _ => None,
        };

        if let Some(ty) = unwrapped_ty {
            &ty.value
        } else {
            self
        }
    }
}
