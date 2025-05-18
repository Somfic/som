use crate::prelude::*;

pub fn parse(
    parser: &mut Parser,
    left: Expression,
    binding_power: BindingPower,
) -> Result<Expression> {
    let token = parser
        .expect(TokenKind::Plus)
        .context("parsing binary expression")?;

    let right = parser.parse_expression(binding_power)?;

    Ok(Expression {
        value: ExpressionValue::Binary(BinaryExpression {
            left: Box::new(left),
            operator: BinaryOperator::Add,
            right: Box::new(right),
        }),
        span: token.span,
    })
}

pub fn type_check(expression: &Expression) -> Result<TypedExpression> {
    let value = match &expression.value {
        ExpressionValue::Binary(value) => value,
        _ => unreachable!(),
    };

    Ok(TypedExpression {
        value: ExpressionValue::Binary(value.clone()),
        span: expression.into(),
        type_: Type::new(expression, TypeKind::Integer),
    })
}
