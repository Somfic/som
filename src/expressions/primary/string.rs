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
    let value = match &expression.value {
        TypedExpressionValue::Primary(PrimaryExpression::String(value)) => value,
        _ => unreachable!(),
    };

    // Create a null-terminated C string from the SOM string literal
    let mut c_string_bytes = value.as_bytes().to_vec();
    c_string_bytes.push(0); // null terminator

    // For JIT compilation, we need to allocate the string in memory that will persist
    // We'll use Box::leak to create a static allocation for the string
    let c_string_boxed = c_string_bytes.into_boxed_slice();
    let static_string: &'static [u8] = Box::leak(c_string_boxed);

    let str_addr = static_string.as_ptr() as i64;

    body.ins().iconst(cranelift::prelude::types::I64, str_addr)
}
