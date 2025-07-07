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
    body: &mut FunctionBuilder,
    env: &mut crate::compiler::Environment,
) -> CompileValue {
    body.ins().iconst(CompilerType::I8, 0)
}
