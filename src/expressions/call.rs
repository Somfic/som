use cranelift::codegen::ir::BlockArg;
use cranelift_module::{FuncId, Module};

use crate::expressions;
pub use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct CallExpression<Expression> {
    pub callee: Box<Expression>,
    pub arguments: Vec<Expression>,
    pub last_argument_offset: usize,
}

pub fn parse(parser: &mut Parser, expression: Expression, bp: BindingPower) -> Result<Expression> {
    let (arguments, span) = parser.expect_list(
        TokenKind::ParenOpen,
        |parser| parser.parse_expression(BindingPower::None),
        TokenKind::Comma,
        TokenKind::ParenClose,
    )?;

    let span = expression.span + span;

    Ok(ExpressionValue::Call(CallExpression {
        callee: Box::new(expression),
        arguments,
        last_argument_offset: span.offset() + span.length() - 1,
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Call(value) => value,
        _ => unreachable!(),
    };

    let callee = type_checker.check_expression(&value.callee, env);

    let function = match &callee.type_.value {
        TypeValue::Function(function) => function,
        TypeValue::Never => {
            // Return a call expression with Never type
            return expression.with_value_type(
                TypedExpressionValue::Call(CallExpression {
                    callee: Box::new(callee),
                    arguments: value
                        .arguments
                        .iter()
                        .map(|argument| type_checker.check_expression(argument, env))
                        .collect(),
                    last_argument_offset: value.last_argument_offset,
                }),
                TypeValue::Never.with_span(expression.span),
            );
        }
        _ => {
            // Add error for trying to call a non-function
            type_checker.add_error(Error::TypeChecker(TypeCheckerError::TypeMismatch {
                help: format!("cannot call {} as a function", callee.type_),
                labels: vec![miette::LabeledSpan::new(
                    Some(format!("this is {} but expected a function", callee.type_)),
                    callee.span.offset(),
                    callee.span.length(),
                )],
            }));

            // Return a call expression with Never type to prevent cascading errors
            return expression.with_value_type(
                TypedExpressionValue::Call(CallExpression {
                    callee: Box::new(callee),
                    arguments: value
                        .arguments
                        .iter()
                        .map(|argument| type_checker.check_expression(argument, env))
                        .collect(),
                    last_argument_offset: value.last_argument_offset,
                }),
                TypeValue::Never.with_span(expression.span),
            );
        }
    };

    let type_ = function.return_type.clone().with_span(expression.span);

    let arguments: Vec<_> = value
        .arguments
        .iter()
        .map(|argument| type_checker.check_expression(argument, env))
        .collect();

    // check arguments
    check_arguments(
        type_checker,
        &arguments,
        &function.parameters,
        value.last_argument_offset,
        function,
    );

    expression.with_value_type(
        TypedExpressionValue::Call(CallExpression {
            callee: Box::new(callee),
            arguments,
            last_argument_offset: value.last_argument_offset,
        }),
        type_,
    )
}

fn check_arguments(
    type_checker: &mut TypeChecker,
    arguments: &[TypedExpression],
    parameters: &[Parameter],
    missing_argument_offset: usize,
    function: &FunctionType,
) {
    if arguments.len() < parameters.len() {
        for parameter in &parameters[arguments.len()..] {
            type_checker.add_error(Error::TypeChecker(TypeCheckerError::MissingParameter {
                help: format!(
                    "missing argument for parameter `{}` of type `{}`",
                    parameter.identifier, parameter.type_
                ),
                argument: (missing_argument_offset, 0),
            }));
        }
    }

    if parameters.len() < arguments.len() {
        for argument in &arguments[parameters.len()..] {
            type_checker.add_error(Error::TypeChecker(TypeCheckerError::UnexpectedArgument {
                help: "remove this argument or add a parameter to the function signature"
                    .to_string(),
                argument: argument.clone(),
                signature: function.span,
            }));
        }
    }

    for (argument, parameter) in arguments.iter().zip(parameters) {
        let argument_type = &argument.type_;
        let parameter_type = &parameter.type_;

        type_checker.expect_type(
            argument_type,
            parameter_type,
            parameter,
            format!("for parameter `{}`", parameter.identifier),
        );
    }
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder<'_>,
    env: &mut crate::compiler::Environment<'_>,
    tail_ctx: crate::compiler::TailContext,
) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Call(value) => value,
        _ => unreachable!(),
    };

    // Helper to extract identifier from callee expression
    fn get_callee_identifier(expr: &TypedExpression) -> Option<&Identifier> {
        match &expr.value {
            TypedExpressionValue::Identifier(id) => Some(id),
            TypedExpressionValue::Group(group) => get_callee_identifier(&group.expression),
            _ => None,
        }
    }

    fn get_func_id(
        compiler: &mut Compiler,
        expression: &TypedExpression,
        env: &mut CompileEnvironment,
    ) -> FuncId {
        match &expression.value {
            TypedExpressionValue::Function(_) => {
                let (func_id, _captured_vars) =
                    expressions::function::compile(compiler, expression, env);
                func_id
            }
            TypedExpressionValue::Group(group) => get_func_id(compiler, &group.expression, env),
            TypedExpressionValue::Identifier(identifier) => env.get_function(identifier).unwrap(),
            _ => panic!("not a function: {expression:?}"),
        }
    }

    let func_id = get_func_id(compiler, &value.callee, env);

    // Check if this is a tail call to the same function
    if let crate::compiler::TailContext::InTail { func_id: current_func, loop_start } = tail_ctx {
        if func_id == current_func {
            // This is a self-tail-call! Update parameters and jump instead of calling

            // Compile arguments to temporary values (can't update params while reading them)
            let arg_values: Vec<cranelift::prelude::Value> = value
                .arguments
                .iter()
                .map(|arg| compiler.compile_expression(arg, body, env))
                .collect();

            // Jump back to loop start with new argument values
            // Create a dummy value before the jump (won't be used but needed for type system)
            let dummy = body.ins().iconst(cranelift::prelude::types::I64, 0);

            // Jump back to loop start, passing new argument values as block parameters
            let block_args: Vec<BlockArg> = arg_values.iter().map(|v| BlockArg::Value(*v)).collect();
            body.ins().jump(loop_start, &block_args);

            // Return the dummy value (unreachable, but satisfies type system)
            return dummy;
        }
    }

    // Not a tail call, do normal function call
    let func_ref = compiler.codebase.declare_func_in_func(func_id, body.func);

    // Compile regular arguments
    let mut arg_values: Vec<cranelift::prelude::Value> = value
        .arguments
        .iter()
        .map(|arg| compiler.compile_expression(arg, body, env))
        .collect();

    // Check if this is a closure call - if so, prepend captured values
    if let Some(identifier) = get_callee_identifier(&value.callee) {
        if let Some((_, captured_vars)) = env.get_closure(identifier.name.to_string()) {
            // Prepend captured values to argument list
            let mut closure_args = Vec::new();

            for (_name, var, _ty) in captured_vars {
                let captured_value = body.use_var(var);
                closure_args.push(captured_value);
            }

            // Prepend captured arguments before regular arguments
            closure_args.extend(arg_values);
            arg_values = closure_args;
        }
    }

    let call = body.ins().call(func_ref, &arg_values);

    body.inst_results(call)[0]
}
