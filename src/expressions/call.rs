pub use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct CallExpression<Expression> {
    pub callee: Box<Expression>,
    pub arguments: Vec<Expression>,
}

pub fn parse(parser: &mut Parser, expression: Expression, bp: BindingPower) -> Result<Expression> {
    parser.expect(TokenKind::ParenOpen, "expected a function call")?;

    let mut arguments = vec![];

    loop {
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::ParenClose)
        }) {
            break;
        }

        if !arguments.is_empty() {
            parser.expect(TokenKind::Comma, "expected a comma between arguments")?;
        }

        let argument = parser.parse_expression(bp)?;
        arguments.push(argument);
    }

    let close = parser.expect(TokenKind::ParenClose, "expected a function call")?;

    let span = expression.span + close.span;

    Ok(ExpressionValue::Call(CallExpression {
        callee: Box::new(expression.with_span(span)),
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

    let type_ = callee.type_.clone().with_span(callee.span);

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
