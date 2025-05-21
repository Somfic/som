use crate::prelude::*;

pub fn parse(
    parser: &mut Parser,
    left: Expression,
    binding_power: BindingPower,
) -> Result<Expression> {
    parser.expect(TokenKind::Slash, "expected a division operator")?;

    let right = parser.parse_expression(binding_power)?;

    Ok(Expression {
        span: left.span + right.span,
        value: ExpressionValue::Binary(BinaryExpression {
            left: Box::new(left),
            operator: BinaryOperator::Divide,
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
        "both sides of the division must be of the same type",
    );

    TypedExpression {
        span: left.span + right.span,
        value: TypedExpressionValue::Binary(BinaryExpression {
            left: Box::new(left),
            operator: BinaryOperator::Divide,
            right: Box::new(right),
        }),
        type_: Type::new(expression, ty),
    }
}

pub fn compile(expression: &TypedExpression, function: &mut FunctionBuilder) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Binary(value) => value,
        _ => unreachable!(),
    };

    let left = compile(&value.left, function);
    let right = compile(&value.right, function);

    function.ins().fdiv(left, right)
}
