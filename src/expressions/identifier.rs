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

    // Get the variable from the environment
    // Captured variables are now passed as function parameters, so this just looks up local vars
    let var = env
        .get_variable(identifier.name.to_string())
        .unwrap_or_else(|| panic!("variable {} not found in environment", identifier.name));
    body.use_var(var)
}
