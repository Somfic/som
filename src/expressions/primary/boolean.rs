use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::Boolean, "expected a boolean literal")?;

    let value = match token.value {
        TokenValue::Boolean(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primary(PrimaryExpression::Boolean(value)).with_span(token))
}

pub fn type_check(expression: &Expression) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Primary(PrimaryExpression::Boolean(value)) => value,
        _ => unreachable!(),
    };

    let type_ = Type::new(expression, TypeValue::Boolean);
    let value = TypedExpressionValue::Primary(PrimaryExpression::Boolean(*value));

    expression.with_value_type(value, type_)
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) {
    let value = match &expression.value {
        TypedExpressionValue::Primary(PrimaryExpression::Boolean(value)) => value,
        _ => unreachable!(),
    };

    body.ins()
        .iconst(CompilerType::I8, if *value { 1 } else { 0 });
}
