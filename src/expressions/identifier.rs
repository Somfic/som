use crate::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct GroupExpression<Expression> {
    pub expression: Box<Expression>,
}

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let identifier = parser.expect_identifier()?;

    let span = identifier.span;

    Ok(ExpressionValue::Identifier(identifier).with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let identifier = match &expression.value {
        ExpressionValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    let type_ = type_checker
        .expect_declaration(identifier, env, "identifier not found")
        .with_span(identifier);

    TypedExpression {
        type_,
        value: TypedExpressionValue::Identifier(identifier.clone()),
        span: identifier.span,
    }
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> cranelift::prelude::Value {
    let identifier = match &expression.value {
        TypedExpressionValue::Identifier(identifier) => identifier,
        _ => unreachable!(),
    };

    let var = env
        .get_variable(identifier)
        .unwrap_or_else(|| panic!("variable {identifier} not found in environment"));

    body.use_var(var)
}
