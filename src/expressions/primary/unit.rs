use crate::prelude::*;

pub fn type_check(expression: &Expression) -> TypedExpression {
    expression.with_value_type(
        TypedExpressionValue::Primary(PrimaryExpression::Unit),
        Type::new(expression, TypeValue::Unit),
    )
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    env: &mut crate::compiler::Environment,
) {
    todo!()
}
