use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::Minus, "expected a negation")?;

    let value = parser.parse_expression(BindingPower::Unary)?;

    let span = token.span + value.span;

    Ok(ExpressionValue::Unary(UnaryExpression {
        operand: Box::new(value),
        operator: UnaryOperator::Negative,
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Unary(value) => value,
        _ => unreachable!(),
    };

    let value = type_checker.check_expression(&value.operand, env);

    // Check that the operand is a numeric type that supports negation
    let is_numeric = |type_value: &TypeValue| matches!(type_value, TypeValue::I32 | TypeValue::I64);
    
    if !is_numeric(&value.type_.value) {
        type_checker.add_error(type_checker_unexpected_type_value(
            "numeric type (i32 or i64)",
            &value.type_,
            "negation can only be applied to numeric types",
        ));
    }

    TypedExpression {
        type_: Type::new(&value, value.type_.value.clone()),
        value: TypedExpressionValue::Unary(UnaryExpression {
            operand: Box::new(value),
            operator: UnaryOperator::Negative,
        }),
        span: expression.span,
    }
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> CompileValue {
    let (operand, operator) = match &expression.value {
        TypedExpressionValue::Unary(UnaryExpression { operand, operator }) => (operand, operator),
        _ => unreachable!(),
    };

    let value = compiler.compile_expression(&operand, body, env);

    body.ins().ineg(value)
}
