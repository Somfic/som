use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::Boolean, "expected a boolean literal")?;

    let value = match token.value {
        TokenValue::Boolean(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression {
        value: ExpressionValue::Primary(PrimaryExpression::Boolean(value)),
        span: token.span,
    })
}

pub fn type_check(expression: &Expression) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Primary(PrimaryExpression::Boolean(value)) => value,
        _ => unreachable!(),
    };

    TypedExpression {
        value: TypedExpressionValue::Primary(PrimaryExpression::Boolean(*value)),
        span: expression.into(),
        type_: Type::new(expression, TypeValue::Boolean),
    }
}

pub fn compile(expression: &TypedExpression, function: &mut FunctionBuilder) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Primary(PrimaryExpression::Boolean(value)) => value,
        _ => unreachable!(),
    };

    function
        .ins()
        .iconst(cranelift::prelude::types::I8, *value as i64)
}
