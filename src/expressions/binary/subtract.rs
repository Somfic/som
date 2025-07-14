use crate::{compiler, expressions::TypedExpressionValue, prelude::*};

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

    // Check that both sides are numeric types
    let is_numeric = |type_value: &TypeValue| matches!(type_value, TypeValue::I32 | TypeValue::I64);

    if !is_numeric(&left.type_.value) {
        type_checker.add_error(type_checker_unexpected_type_value(
            "numeric type (i32 or i64)",
            &left.type_,
            "only numeric types can be subtracted",
        ));
    }

    if !is_numeric(&right.type_.value) {
        type_checker.add_error(type_checker_unexpected_type_value(
            "numeric type (i32 or i64)",
            &right.type_,
            "only numeric types can be subtracted",
        ));
    }

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

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Binary(value) => value,
        _ => unreachable!(),
    };

    let left = compiler.compile_expression(&value.left, body, env);
    let right = compiler.compile_expression(&value.right, body, env);

    body.ins().isub(left, right)
}
