use crate::prelude::*;
use cranelift::{
    codegen::ir::{Function, UserFuncName},
    prelude::{AbiParam, FunctionBuilderContext, Signature},
};
use cranelift_module::{FuncId, Linkage, Module};
use std::hash::Hash;

#[derive(Debug, Clone)]
pub struct Argument<Expression> {
    pub identifier: Identifier,
    pub value: Expression,
}

#[derive(Debug, Clone)]
pub struct FunctionExpression<Expression> {
    pub parameters: Vec<Parameter>,
    pub explicit_return_type: Option<Type>,
    pub body: Box<Expression>,
    pub span: Span,
}

#[derive(Debug, Clone, Eq)]
pub struct Parameter {
    pub identifier: Identifier,
    pub type_: Box<Type>,
    pub span: Span,
}

impl PartialEq for Parameter {
    fn eq(&self, other: &Self) -> bool {
        self.type_ == other.type_
    }
}

impl Hash for Parameter {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.type_.hash(state);
    }
}

impl From<Parameter> for Span {
    fn from(parameter: Parameter) -> Self {
        parameter.span
    }
}

impl From<&Parameter> for Span {
    fn from(parameter: &Parameter) -> Self {
        parameter.span
    }
}

impl From<Parameter> for miette::SourceSpan {
    fn from(parameter: Parameter) -> Self {
        parameter.span.into()
    }
}

impl From<&Parameter> for miette::SourceSpan {
    fn from(parameter: &Parameter) -> Self {
        parameter.span.into()
    }
}

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let start = parser.expect(TokenKind::Function, "expected a function signature")?;

    parser.expect(TokenKind::ParenOpen, "expected function arguments")?;
    let mut parameters = vec![];

    loop {
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::ParenClose)
        }) {
            break;
        }

        if !parameters.is_empty() {
            parser.expect(TokenKind::Comma, "expected a comma between arguments")?;
        }

        let identifier = parser.expect_identifier()?;

        parser.expect(
            TokenKind::Tilde,
            format!("expected a parameter type for `{}`", identifier.name),
        )?;

        let type_ = parser.parse_type(BindingPower::None)?;

        parameters.push(Parameter {
            span: identifier.span + type_.span,
            identifier,
            type_: Box::new(type_),
        });
    }

    let end = parser.expect(TokenKind::ParenClose, "expected function arguments")?;

    // Optional return type
    let explicit_return_type = if parser.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::Arrow)
    }) {
        parser.expect(TokenKind::Arrow, "expected return type")?;
        Some(parser.parse_type(BindingPower::None)?)
    } else {
        None
    };

    let body = parser.parse_expression(BindingPower::None)?;

    let span = start.span + body.span;

    Ok(ExpressionValue::Function(FunctionExpression {
        parameters,
        explicit_return_type,
        body: Box::new(body),
        span: start.span + end.span,
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Function(value) => value,
        _ => unreachable!(),
    };

    let env = &mut env.block();

    for parameter in &value.parameters {
        env.set(&parameter.identifier, &parameter.type_);
    }

    let body = type_checker.check_expression(&value.body, env);

    let type_ = TypeValue::Function(FunctionType {
        parameters: value.parameters.clone(),
        returns: Box::new(body.type_.clone()),
        span: value.span,
    });

    if let Some(explicit_return_type) = &value.explicit_return_type {
        type_checker.expect_same_type(
            vec![&body.type_, explicit_return_type],
            "the function's body should match the explicit return type",
        );
    }

    let value = TypedExpressionValue::Function(FunctionExpression {
        parameters: value.parameters.clone(),
        body: Box::new(body),
        explicit_return_type: value.explicit_return_type.clone(),
        span: value.span,
    });

    expression.with_value_type(value, Type::new(expression, type_))
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    env: &mut CompileEnvironment,
) -> FuncId {
    let value = match &expression.value {
        TypedExpressionValue::Function(value) => value,
        _ => unreachable!(),
    };

    let mut signature = Signature::new(compiler.isa.default_call_conv());
    for parameter in &value.parameters {
        signature
            .params
            .push(AbiParam::new(parameter.type_.value.to_ir()));
    }

    signature
        .returns
        .push(AbiParam::new(value.body.type_.value.to_ir()));

    let func_id = compiler
        .codebase
        .declare_anonymous_function(&signature) // Anonymous function
        .unwrap();

    let mut context = compiler.codebase.make_context();
    context.func = Function::new();
    context.func.signature = signature;

    let mut function_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut context.func, &mut function_context);

    let body_block = builder.create_block();
    builder.append_block_params_for_function_params(body_block);
    builder.switch_to_block(body_block);

    let block_params = builder.block_params(body_block).to_vec();
    for (i, parameter) in value.parameters.iter().enumerate() {
        let variable =
            env.declare_variable(&parameter.identifier, &mut builder, &parameter.type_.value);
        builder.def_var(variable, block_params[i]);
    }

    let body = compiler.compile_expression(&value.body, &mut builder, env);

    builder.ins().return_(&[body]);
    builder.seal_block(body_block);
    builder.finalize();

    compiler
        .codebase
        .define_function(func_id, &mut context)
        .unwrap();

    func_id
}
