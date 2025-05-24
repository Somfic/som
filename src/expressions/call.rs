pub use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct CallExpression<Expression> {
    pub callee: Box<Expression>,
    pub arguments: Vec<Expression>,
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
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut Environment,
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

    let type_ = function.returns.clone().with_span(expression.span);

    let arguments = value
        .arguments
        .iter()
        .map(|argument| type_checker.check_expression(argument, env))
        .collect();

    expression.with_value_type(
        TypedExpressionValue::Call(CallExpression {
            callee: Box::new(callee),
            arguments,
        }),
        type_,
    )
}
