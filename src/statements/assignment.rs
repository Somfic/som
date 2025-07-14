use crate::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentStatement<Expression> {
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

    // For now, we'll return this as an expression that wraps an assignment statement
    // This is a bit of a hack, but it allows us to use the left expression handler system
    let assignment_statement = StatementValue::Assignment(AssignmentStatement {
        identifier,
        value: Box::new(value.clone()),
    })
    .with_span(span);

    // We need to return an expression, so we'll wrap it as an expression statement
    // This will be handled specially in the block parser
    Ok(ExpressionValue::Statement(assignment_statement).with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    statement: &Statement,
    env: &mut crate::type_checker::Environment,
) -> TypedStatement {
    let assignment = match &statement.value {
        StatementValue::Assignment(assignment) => assignment,
        _ => unreachable!(),
    };

    let value = type_checker.check_expression(&assignment.value, env);

    // Check that the variable exists in the environment
    let variable_type = env
        .get_variable(&assignment.identifier.name)
        .ok_or_else(|| {
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

    TypedStatement {
        value: StatementValue::Assignment(AssignmentStatement {
            identifier: assignment.identifier.clone(),
            value: Box::new(value),
        }),
        span: statement.span,
    }
}

pub fn compile(
    compiler: &mut Compiler,
    statement: &TypedStatement,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) {
    let assignment = match &statement.value {
        StatementValue::Assignment(assignment) => assignment,
        _ => unreachable!(),
    };

    let value = compiler.compile_expression(&assignment.value, body, env);

    // Get the variable from the environment
    if let Some(var) = env.get_variable(&assignment.identifier.name) {
        body.def_var(var, value);
    }
}
