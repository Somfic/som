use crate::prelude::*;

pub fn type_check(expression: &Expression) -> TypedExpression {
    expression.with_value_type(
        TypedExpressionValue::Primary(PrimaryExpression::Unit),
        Type::new(expression, TypeValue::Unit),
    )
}
