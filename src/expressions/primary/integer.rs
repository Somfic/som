use cranelift::prelude::{FunctionBuilder, InstBuilder};

use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::Integer, "expected an integer literal")?;

    let value = match token.value {
        TokenValue::Integer(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primary(PrimaryExpression::Integer(value)).with_span(token.span))
}

pub fn type_check(expression: &Expression) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Primary(PrimaryExpression::Integer(value)) => value,
        _ => unreachable!(),
    };

    let value = TypedExpressionValue::Primary(PrimaryExpression::Integer(*value));
    let type_ = Type::new(expression, TypeValue::Integer);

    expression.with_value_type(value, type_)
}

pub fn compile(expression: &TypedExpression, function: &mut FunctionBuilder) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Primary(PrimaryExpression::Integer(value)) => value,
        _ => unreachable!(),
    };

    function
        .ins()
        .iconst(cranelift::prelude::types::I64, *value)
}
