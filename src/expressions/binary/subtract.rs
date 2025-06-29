use crate::{expressions::TypedExpressionValue, prelude::*};

pub fn parse(
    parser: &mut Parser,
    left: Expression,
    binding_power: BindingPower,
) -> Result<Expression> {
    parser.expect(TokenKind::Minus, "expected a minus operator")?;

    let right = parser.parse_expression(binding_power)?;

    let span = left.span + right.span;

    Ok(ExpressionValue::Binary(BinaryExpression {
        left: Box::new(left),
        operator: BinaryOperator::Subtract,
        right: Box::new(right),
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Binary(value) => value,
        _ => unreachable!(),
    };

    let left = type_checker.check_expression(&value.left, env);
    let right = type_checker.check_expression(&value.right, env);

    let ty = type_checker.expect_same_type(
        vec![&left.type_, &right.type_],
        "both sides of the subtraction must be of the same type",
    );

    let value = TypedExpressionValue::Binary(BinaryExpression {
        left: Box::new(left),
        operator: BinaryOperator::Subtract,
        right: Box::new(right),
    });
    let type_ = Type::new(expression, ty);

    expression.with_value_type(value, type_)
}

pub fn compile(expression: &TypedExpression, function: &mut FunctionBuilder) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Binary(value) => value,
        _ => unreachable!(),
    };

    let left = compile(&value.left, function);
    let right = compile(&value.right, function);

    function.ins().isub(left, right)
}
