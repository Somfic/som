use crate::prelude::*;
use cranelift::{
    codegen::ir::Function,
    prelude::{AbiParam, FunctionBuilderContext, Signature},
};
use cranelift_module::{FuncId, Module};
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

    let (parameters, parameters_span) = parse_parameters(parser)?;

    // Return type is mandatory
    parser.expect(
        TokenKind::Arrow,
        "expected an arrow (->) followed by a return type",
    )?;
    let explicit_return_type = Some(parser.parse_type(BindingPower::None)?);

    let body = parser.parse_expression(BindingPower::None)?;

    let span = start.span + body.span;

    Ok(ExpressionValue::Function(FunctionExpression {
        parameters,
        explicit_return_type,
        body: Box::new(body),
        span: start.span + parameters_span,
    })
    .with_span(span))
}

pub fn parse_parameters(parser: &mut Parser) -> Result<(Vec<Parameter>, Span)> {
    let mut parameters = vec![];

    let start = parser.expect(TokenKind::ParenOpen, "expected function parameters")?;

    loop {
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::ParenClose)
        }) {
            break;
        }

        if !parameters.is_empty() {
            parser.expect(TokenKind::Comma, "expected a comma between parameters")?;
        }

        let identifier = parser.expect_identifier()?;

        parser.expect(
            TokenKind::Tilde,
            format!("expected a type for `{}`", identifier.name),
        )?;

        let type_ = parser.parse_type(BindingPower::None)?;

        parameters.push(Parameter {
            span: identifier.span + type_.span,
            identifier,
            type_: Box::new(type_),
        });
    }

    let end = parser.expect(TokenKind::ParenClose, "expected a closing parenthesis")?;

    Ok((parameters, start.span + end.span))
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

    // Create a function environment that inherits from the current environment (for closures)
    let mut function_env = env.function();
    //let env: &mut crate::type_checker::Environment<'_> = &mut env.block();

    for parameter in &value.parameters {
        function_env.declare(&parameter.identifier, &parameter.type_);
    }

    let body = type_checker.check_expression(&value.body, &mut function_env);

    let type_ = TypeValue::Function(FunctionType {
        parameters: value.parameters.clone(),
        return_type: Box::new(body.type_.clone()),
        span: value.span,
    });

    if let Some(explicit_return_type) = &value.explicit_return_type {
        type_checker.expect_same_type(
            vec![&body.type_, explicit_return_type],
            "the function's body should match it's explicit return type",
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
) -> (
    FuncId,
    Vec<(String, cranelift::prelude::Variable, TypeValue)>,
) {
    let value = match &expression.value {
        TypedExpressionValue::Function(value) => value,
        _ => unreachable!(),
    };

    // We need a type environment to do capture analysis
    // For now, we'll identify captures by checking what variables are used in the body
    // that aren't parameters or locally declared
    let captured_vars = analyze_captures(&value, env);

    let mut signature = Signature::new(compiler.isa.default_call_conv());

    // Add captured variables as leading parameters
    for (_name, _var, ty) in &captured_vars {
        signature.params.push(AbiParam::new(ty.to_ir()));
    }

    // Add regular parameters
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

    // Create a new environment for this function to isolate parameter declarations
    let mut function_env = env.block();

    let block_params = builder.block_params(body_block).to_vec();
    let mut param_index = 0;

    // Bind captured variables first
    for (name, _var, ty) in &captured_vars {
        let variable = function_env.declare_variable(name, &mut builder, ty);
        builder.def_var(variable, block_params[param_index]);
        param_index += 1;
    }

    // Then bind regular parameters
    for parameter in value.parameters.iter() {
        let variable = function_env.declare_variable(
            &parameter.identifier,
            &mut builder,
            &parameter.type_.value,
        );
        builder.def_var(variable, block_params[param_index]);
        param_index += 1;
    }

    // Compile the body - captured variables are now available as local variables
    let body = compiler.compile_expression(&value.body, &mut builder, &mut function_env);

    builder.ins().return_(&[body]);
    builder.seal_block(body_block);
    builder.finalize();

    if let Err(error) = compiler.codebase.define_function(func_id, &mut context) {
        println!("{:#?}", error);
    };

    (func_id, captured_vars)
}

/// Compile a function with an optional name (for recursion support)
pub fn compile_with_name(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    env: &mut CompileEnvironment,
    name: Option<&Identifier>,
) -> (
    FuncId,
    Vec<(String, cranelift::prelude::Variable, TypeValue)>,
) {
    let value = match &expression.value {
        TypedExpressionValue::Function(value) => value,
        _ => unreachable!(),
    };

    // Analyze captures first
    let captured_vars = analyze_captures(&value, env);

    // Create signature
    let mut signature = Signature::new(compiler.isa.default_call_conv());

    // Add captured variables as leading parameters
    for (_name, _var, ty) in &captured_vars {
        signature.params.push(AbiParam::new(ty.to_ir()));
    }

    // Add regular parameters
    for parameter in &value.parameters {
        signature
            .params
            .push(AbiParam::new(parameter.type_.value.to_ir()));
    }

    signature
        .returns
        .push(AbiParam::new(value.body.type_.value.to_ir()));

    // Create function ID early
    let func_id = compiler
        .codebase
        .declare_anonymous_function(&signature)
        .unwrap();

    // If this is a named function, pre-register it for recursion
    let _guard = if let Some(name) = name {
        if captured_vars.is_empty() {
            env.declare_function(name, func_id);
        }
        Some(())  // Guard to ensure we don't accidentally remove it
    } else {
        None
    };

    // Now compile the function body (which can now call itself recursively)
    let mut context = compiler.codebase.make_context();
    context.func = Function::new();
    context.func.signature = signature;

    let mut function_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut context.func, &mut function_context);

    let body_block = builder.create_block();
    builder.append_block_params_for_function_params(body_block);
    builder.switch_to_block(body_block);

    // Create a new environment for this function to isolate parameter declarations
    let mut function_env = env.block();

    let block_params = builder.block_params(body_block).to_vec();
    let mut param_index = 0;

    // Bind captured variables first
    for (name, _var, ty) in &captured_vars {
        let variable = function_env.declare_variable(name, &mut builder, ty);
        builder.def_var(variable, block_params[param_index]);
        param_index += 1;
    }

    // Then bind regular parameters
    for parameter in value.parameters.iter() {
        let variable = function_env.declare_variable(
            &parameter.identifier,
            &mut builder,
            &parameter.type_.value,
        );
        builder.def_var(variable, block_params[param_index]);
        param_index += 1;
    }

    // Compile the body - captured variables are now available as local variables
    let body = compiler.compile_expression(&value.body, &mut builder, &mut function_env);

    builder.ins().return_(&[body]);
    builder.seal_block(body_block);
    builder.finalize();

    if let Err(error) = compiler.codebase.define_function(func_id, &mut context) {
        println!("{:#?}", error);
    };

    (func_id, captured_vars)
}

/// Analyze what variables a function captures from its environment
fn analyze_captures(
    function: &FunctionExpression<TypedExpression>,
    env: &CompileEnvironment,
) -> Vec<(String, cranelift::prelude::Variable, TypeValue)> {
    let mut captured = Vec::new();
    let mut local_vars = std::collections::HashSet::new();

    // Add parameters to local variables
    for param in &function.parameters {
        local_vars.insert(param.identifier.name.to_string());
    }

    // Find all identifiers used in the function body
    collect_captures_from_expr(&function.body, &mut captured, &local_vars, env);

    captured
}

fn collect_captures_from_expr(
    expr: &TypedExpression,
    captured: &mut Vec<(String, cranelift::prelude::Variable, TypeValue)>,
    local_vars: &std::collections::HashSet<String>,
    env: &CompileEnvironment,
) {
    match &expr.value {
        TypedExpressionValue::Identifier(identifier) => {
            let name = identifier.name.to_string();
            // If it's not a local variable and we haven't already captured it
            if !local_vars.contains(&name) && !captured.iter().any(|(n, _, _)| n == &name) {
                // Try to get it from the environment
                if let Some((var, ty)) = env.get_variable_with_type(&name) {
                    captured.push((name, var, ty.clone()));
                }
            }
        }
        TypedExpressionValue::Block(block) => {
            let mut block_locals = local_vars.clone();
            for stmt in &block.statements {
                if let StatementValue::VariableDeclaration(decl) = &stmt.value {
                    block_locals.insert(decl.identifier.name.to_string());
                    collect_captures_from_expr(&decl.value, captured, &block_locals, env);
                }
            }
            collect_captures_from_expr(&block.result, captured, &block_locals, env);
        }
        TypedExpressionValue::Binary(binary) => {
            collect_captures_from_expr(&binary.left, captured, local_vars, env);
            collect_captures_from_expr(&binary.right, captured, local_vars, env);
        }
        TypedExpressionValue::Unary(unary) => {
            collect_captures_from_expr(&unary.operand, captured, local_vars, env);
        }
        TypedExpressionValue::Call(call) => {
            collect_captures_from_expr(&call.callee, captured, local_vars, env);
            for arg in &call.arguments {
                collect_captures_from_expr(arg, captured, local_vars, env);
            }
        }
        TypedExpressionValue::Conditional(cond) => {
            collect_captures_from_expr(&cond.condition, captured, local_vars, env);
            collect_captures_from_expr(&cond.truthy, captured, local_vars, env);
            collect_captures_from_expr(&cond.falsy, captured, local_vars, env);
        }
        TypedExpressionValue::Function(_) => {
            // Nested functions would need their own analysis
            // For now, skip them
        }
        TypedExpressionValue::Group(group) => {
            collect_captures_from_expr(&group.expression, captured, local_vars, env);
        }
        TypedExpressionValue::Assignment(assignment) => {
            collect_captures_from_expr(&assignment.value, captured, local_vars, env);
        }
        TypedExpressionValue::FieldAccess(field) => {
            collect_captures_from_expr(&field.object, captured, local_vars, env);
        }
        TypedExpressionValue::StructConstructor(strukt) => {
            for arg in &strukt.arguments {
                collect_captures_from_expr(&arg.value, captured, local_vars, env);
            }
        }
        TypedExpressionValue::WhileLoop(while_loop) => {
            collect_captures_from_expr(&while_loop.condition, captured, local_vars, env);
            collect_captures_from_expr(&while_loop.body, captured, local_vars, env);
        }
        TypedExpressionValue::Primary(_) => {
            // Literals don't capture anything
        }
    }
}
