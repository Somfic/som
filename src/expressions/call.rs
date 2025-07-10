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
        _ => {
            panic!("not a function: {}", callee.type_);
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
                    "supply a value for `{}` ({})",
                    parameter.identifier, parameter.type_
                ),
                argument: (missing_argument_offset, 0),
                parameter: parameter.clone(),
            }));
        }
    }

    if parameters.len() < arguments.len() {
        for argument in &arguments[parameters.len()..] {
            type_checker.add_error(Error::TypeChecker(TypeCheckerError::UnexpectedArgument {
                help: "remove this argument or add a parameter to the function signature"
                    .to_string(),
                argument: argument.clone(),
                function: function.clone(),
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
) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Call(value) => value,
        _ => unreachable!(),
    };

    fn get_func_id(
        compiler: &mut Compiler,
        expression: &TypedExpression,
        env: &mut CompileEnvironment,
    ) -> FuncId {
        match &expression.value {
            TypedExpressionValue::Function(_) => {
                expressions::function::compile(compiler, expression, env)
            }
            TypedExpressionValue::Group(group) => get_func_id(compiler, &group.expression, env),
            TypedExpressionValue::Identifier(identifier) => env.get_function(identifier).unwrap(),
            _ => panic!("not a function: {:?}", expression),
        }
    }

    let func_id = get_func_id(compiler, &value.callee, env);

    let func_ref = compiler.codebase.declare_func_in_func(func_id, body.func);

    let arg_values = value
        .arguments
        .iter()
        .map(|arg| compiler.compile_expression(arg, body, env))
        .collect::<Vec<_>>();

    let call = body.ins().call(func_ref, &arg_values);

    body.inst_results(call)[0]
}
