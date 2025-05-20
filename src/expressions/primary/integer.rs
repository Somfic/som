use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::Integer, "expected an integer literal")?;

    let value = match token.value {
        TokenValue::Integer(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression {
        value: ExpressionValue::Primary(PrimaryExpression::Integer(value)),
        span: token.span,
    })
}

pub fn type_check(expression: &Expression) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Primary(PrimaryExpression::Integer(value)) => value,
        _ => unreachable!(),
    };

    TypedExpression {
        value: ExpressionValue::Primary(PrimaryExpression::Integer(*value)),
        span: expression.into(),
        type_: Type::new(expression, TypeValue::Integer),
    }
}
