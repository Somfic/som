use crate::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentExpression<Expression> {
    pub identifier: Identifier,
    pub value: Box<Expression>,
}

pub fn parse(
    parser: &mut Parser,
    identifier_expr: Expression,
    _binding_power: BindingPower,
) -> Result<Expression> {
    let identifier = match identifier_expr.value {
        ExpressionValue::Identifier(identifier) => identifier,
        _ => return Err(parser_expected_identifier(identifier_expr.span)),
    };

    parser.expect(TokenKind::Equal, "expected assignment operator")?;

    let value = parser.parse_expression(BindingPower::Assignment)?;

    let span = identifier.span + value.span;

    Ok(ExpressionValue::Assignment(AssignmentExpression {
        identifier,
        value: Box::new(value),
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let assignment = match &expression.value {
        ExpressionValue::Assignment(assignment) => assignment,
        _ => unreachable!(),
    };

    let value = type_checker.check_expression(&assignment.value, env);

    // Check that the variable exists in the environment
    let variable_type = env.get(&assignment.identifier).ok_or_else(|| {
        declaration_not_found(
            &assignment.identifier,
            format!("variable `{}` not found", assignment.identifier.name),
            env,
        )
    });

    match variable_type {
        Ok(var_type) => {
            type_checker.expect_same_type(
                vec![&var_type, &value.type_],
                format!(
                    "cannot assign {} to variable `{}` of type {}",
                    value.type_.value, assignment.identifier.name, var_type.value
                ),
            );
        }
        Err(err) => {
            type_checker.add_error(err);
        }
    }

    // Assignment returns the assigned value
    let type_ = value.type_.clone().with_span(expression.span);

    let typed_assignment = TypedExpressionValue::Assignment(AssignmentExpression {
        identifier: assignment.identifier.clone(),
        value: Box::new(value),
    });

    expression.with_value_type(typed_assignment, type_)
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> CompileValue {
    let assignment = match &expression.value {
        TypedExpressionValue::Assignment(assignment) => assignment,
        _ => unreachable!(),
    };

    let value = compiler.compile_expression(&assignment.value, body, env);

    // Get the variable from the environment
    if let Some(var) = env.get_variable(assignment.identifier.name.as_ref()) {
        body.def_var(var, value);
    }

    // Return the assigned value
    value
}
