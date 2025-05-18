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

pub fn type_check(expression: &Expression) -> Result<TypedExpression> {
    let value = match &expression.value {
        ExpressionValue::Primary(PrimaryExpression::Boolean(value)) => value,
        _ => unreachable!(),
    };

    Ok(TypedExpression {
        value: ExpressionValue::Primary(PrimaryExpression::Boolean(*value)),
        span: expression.into(),
        type_: Type::new(expression, TypeKind::Boolean),
    })
}
