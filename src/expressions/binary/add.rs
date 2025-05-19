use crate::prelude::*;

pub fn parse(
    parser: &mut Parser,
    left: Expression,
    binding_power: BindingPower,
) -> Result<Expression> {
    let token = parser.expect(TokenKind::Plus, "expected a plus operator")?;

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

pub fn type_check(type_checker: &mut TypeChecker, expression: &Expression) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Binary(value) => value,
        _ => unreachable!(),
    };

    let left = type_checker.check_expression(&value.left);
    let right = type_checker.check_expression(&value.right);

    let ty = type_checker.expect_same_type(
        vec![&left.type_, &right.type_],
        "both sides of the addition operator must be of the same type",
    );

    TypedExpression {
        value: ExpressionValue::Binary(value.clone()),
        span: expression.into(),
        type_: Type::new(expression, ty),
    }
}
