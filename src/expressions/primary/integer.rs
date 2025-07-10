use cranelift::prelude::{FunctionBuilder, InstBuilder};

use crate::prelude::*;

pub fn parse_i32(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::I32, "expected an 32 bit integer literal")?;

    let value = match token.value {
        TokenValue::I32(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primary(PrimaryExpression::I32(value)).with_span(token.span))
}

pub fn parse_i64(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::I64, "expected an 64 bit integer literal")?;

    let value = match token.value {
        TokenValue::I64(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primary(PrimaryExpression::I64(value)).with_span(token.span))
}

pub fn type_check_i32(expression: &Expression) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Primary(PrimaryExpression::I32(value)) => value,
        _ => unreachable!(),
    };

    let value = TypedExpressionValue::Primary(PrimaryExpression::I32(*value));
    let type_ = Type::new(expression, TypeValue::I32);

    expression.with_value_type(value, type_)
}

pub fn type_check_i64(expression: &Expression) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Primary(PrimaryExpression::I64(value)) => value,
        _ => unreachable!(),
    };

    let value = TypedExpressionValue::Primary(PrimaryExpression::I64(*value));
    let type_ = Type::new(expression, TypeValue::I64);

    expression.with_value_type(value, type_)
}

pub fn compile_i32(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Primary(PrimaryExpression::I32(value)) => value,
        _ => unreachable!(),
    };

    body.ins().iconst(CompilerType::I32, *value as i64)
}

pub fn compile_i64(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Primary(PrimaryExpression::I64(value)) => value,
        _ => unreachable!(),
    };

    body.ins().iconst(CompilerType::I64, *value)
}
