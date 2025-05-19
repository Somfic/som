use crate::prelude::*;

pub fn parse(
    parser: &mut Parser,
    left: Expression,
    binding_power: BindingPower,
) -> Result<Expression> {
    parser.expect(TokenKind::Minus, "expected a minus operator")?;

    let right = parser.parse_expression(binding_power)?;

    Ok(Expression {
        span: left.span + right.span,
        value: ExpressionValue::Binary(BinaryExpression {
            left: Box::new(left),
            operator: BinaryOperator::Subtract,
            right: Box::new(right),
        }),
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
        "both sides of the subtraction must be of the same type",
    );

    TypedExpression {
        value: ExpressionValue::Binary(value.clone()),
        span: left.span + right.span,
        type_: Type::new(expression, ty),
    }
}
