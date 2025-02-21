use crate::{
    ast::{BinaryOperator, ExpressionValue, Primitive, TypedExpression},
    prelude::*,
};
use cranelift::{
    codegen::{
        control::ControlPlane,
        ir::{Function, UserFuncName},
        verifier::VerifierError,
        CompileError, CompiledCode,
    },
    prelude::*,
};
use cranelift_module::Module;
use jit::Jit;

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

    pub fn compile(&mut self) -> CompilerResult<CompiledCode> {
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

        println!("{}", self.jit.ctx.func);

        let mut ctrl_plane = ControlPlane::default();
        self.jit
            .ctx
            .compile(self.jit.module.isa(), &mut ctrl_plane)
            .map_err(parse_error)
            .cloned()
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
                    BinaryOperator::Subtract => builder.ins().isub(left_val, right_val),
                    BinaryOperator::Multiply => builder.ins().imul(left_val, right_val),
                    BinaryOperator::Divide => builder.ins().udiv(left_val, right_val),
                    _ => unimplemented!(),
                }
            }
        }
    }

    fn compile_primitive(primitive: &Primitive<'ast>, builder: &mut FunctionBuilder) -> Value {
        match primitive {
            Primitive::Integer(v) => builder.ins().iconst(types::I64, *v),
            Primitive::Decimal(v) => builder.ins().f64const(*v),
            _ => unimplemented!(),
        }
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
