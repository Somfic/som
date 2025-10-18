use cranelift::prelude::FunctionBuilder;

use crate::prelude::*;

#[derive(Debug, Clone)]

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
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Group(GroupExpression { expression }) => expression,
        _ => unreachable!(),
    };

    let mut env = env.block();

    let value = type_checker.check_expression(value, &mut env);

    TypedExpression {
        type_: Type::new(expression, value.type_.value.clone()),
        value: TypedExpressionValue::Group(GroupExpression {
            expression: Box::new(value),
        }),
        span: expression.span.clone(),
    }
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> CompileValue {
    let group = match &expression.value {
        TypedExpressionValue::Group(group) => group,
        _ => unreachable!(),
    };

    // A group expression simply compiles to the result of its inner expression
    compiler.compile_expression(&group.expression, body, env)
}
