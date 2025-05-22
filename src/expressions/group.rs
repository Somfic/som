use cranelift::prelude::{FunctionBuilder, InstBuilder};

use crate::{prelude::*, type_checker::environment::Environment};

#[derive(Debug, Clone, PartialEq)]

pub struct GroupExpression<Expression> {
    pub expression: Box<Expression>,
}

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let start = parser.expect(TokenKind::ParenOpen, "expected the start of the group")?;

    let expression = parser.parse_expression(BindingPower::None)?;

    let end = parser.expect(TokenKind::ParenClose, "expected the end of the group")?;

    let span = start.span + expression.span + end.span;

    Ok(ExpressionValue::Group(GroupExpression {
        expression: Box::new(expression),
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut Environment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Group(GroupExpression { expression }) => expression,
        _ => unreachable!(),
    };

    let mut env = env.block();

    let value = type_checker.check_expression(value, &mut env);

    TypedExpression {
        type_: Type::new(expression, value.type_.value),
        value: TypedExpressionValue::Group(GroupExpression {
            expression: Box::new(value),
        }),
        span: expression.span.clone(),
    }
}

pub fn compile(expression: &TypedExpression, function: &mut FunctionBuilder) -> CompileValue {
    todo!("implement group expression compilation");
}
