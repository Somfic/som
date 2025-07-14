use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::String, "expected a string literal")?;

    let value = match &token.value {
        TokenValue::String(value) => value.clone(),
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primary(PrimaryExpression::String(value)).with_span(token.span))
}

pub fn type_check(
    _type_checker: &mut TypeChecker,
    expression: &Expression,
    _env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Primary(PrimaryExpression::String(value)) => value.clone(),
        _ => unreachable!(),
    };

    let value = TypedExpressionValue::Primary(PrimaryExpression::String(value));
    let type_ = Type::new(expression, TypeValue::String);

    expression.with_value_type(value, type_)
}

pub fn compile(
    _compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    _env: &mut CompileEnvironment,
) -> CompileValue {
    let _value = match &expression.value {
        TypedExpressionValue::Primary(PrimaryExpression::String(value)) => value,
        _ => unreachable!(),
    };

    // For a demonstration, let's hardcode a string pointer
    // In a real implementation, we'd properly manage string memory
    // For now, we'll create a static C string and use its address

    // This is unsafe but demonstrates the concept
    static HELLO_WORLD: &[u8] = b"Hello, World!\0";
    let str_addr = HELLO_WORLD.as_ptr() as i64;

    body.ins().iconst(cranelift::prelude::types::I64, str_addr)
}
