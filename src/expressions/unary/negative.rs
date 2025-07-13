use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::Minus, "expected a negation")?;

    let value = parser.parse_expression(BindingPower::Additive)?;

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

    // TODO: we should suppot both i32 and i64 for negation
    type_checker.expect_type_value(
        &value.type_,
        &TypeValue::I32,
        "negation can only be applied to i32 or i64 types",
    );

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
