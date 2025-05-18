use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::Integer)?;

    let value = match token.value {
        TokenValue::Integer(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression {
        value: ExpressionValue::Primary(PrimaryExpression::Integer(value)),
        span: token.span,
    })
}

pub fn type_check(expression: &Expression) -> Result<TypedExpression> {
    let value = match &expression.value {
        ExpressionValue::Primary(value) => value,
        _ => unreachable!(),
    };

    Ok(TypedExpression {
        value: ExpressionValue::Primary(value.clone()),
        span: expression.into(),
        type_: Type::new(expression, TypeKind::Integer),
    })
}
