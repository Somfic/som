use crate::{expressions, prelude::*};

#[derive(Debug, Clone, PartialEq)]
pub struct VariableDeclarationStatement<Expression> {
    pub identifier: Identifier,
    pub explicit_type: Option<Type>,
    pub value: Box<Expression>,
}

pub fn parse(parser: &mut Parser) -> Result<Statement> {
    let token = parser.expect(TokenKind::Let, "expected a variable declaration")?;

    let identifier = parser.expect_identifier()?;

    let explicit_type = if parser.peek().is_some_and(|token| {
        token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::Tilde)
    }) {
        parser.expect(TokenKind::Tilde, "expected a type")?;
        Some(parser.parse_type(BindingPower::None)?)
    } else {
        None
    };

    parser.expect(TokenKind::Equal, "expected a value")?;

    let value = parser.parse_expression(BindingPower::Assignment)?;

    let span = token.span + identifier.span + value.span;

    Ok(
        StatementValue::VariableDeclaration(VariableDeclarationStatement {
            identifier,
            explicit_type,
            value: Box::new(value),
        })
        .with_span(span),
    )
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    statement: &Statement,
    env: &mut crate::type_checker::Environment,
) -> TypedStatement {
    let declaration = match &statement.value {
        StatementValue::VariableDeclaration(declaration) => declaration,
        _ => unreachable!(),
    };

    // Special case for function declarations - pass the function name for recursion
    let value = if let ExpressionValue::Function(_) = &declaration.value.value {
        type_checker.check_function_with_name(&declaration.value, env, &declaration.identifier)
    } else {
        type_checker.check_expression(&declaration.value, env)
    };

    if let Some(explicit_type) = &declaration.explicit_type {
        type_checker.expect_same_type(
            vec![&value.type_, explicit_type],
            "the variable should match its explicit type",
        );
    }

    env.declare(&declaration.identifier, &value.type_);

    TypedStatement {
        value: StatementValue::VariableDeclaration(VariableDeclarationStatement {
            identifier: declaration.identifier.clone(),
            explicit_type: declaration.explicit_type.clone(),
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
    let declaration = match &statement.value {
        StatementValue::VariableDeclaration(declaration) => declaration,
        _ => unreachable!(),
    };

    match &declaration.value.value {
        TypedExpressionValue::Function(_) => {
            let (func_id, captured_vars) =
                expressions::function::compile_with_name(compiler, &declaration.value, env, Some(&declaration.identifier));

            // If the function captures variables, store it as a closure
            // Otherwise store it as a plain function (ZST)
            if captured_vars.is_empty() {
                env.declare_function(&declaration.identifier, func_id);
            } else {
                env.declare_closure(&declaration.identifier, func_id, captured_vars);
            }
        }
        _ => {
            let var = env.declare_variable(
                &declaration.identifier,
                body,
                &declaration.value.type_.value,
            );
            let value = compiler.compile_expression(&declaration.value, body, env);
            body.def_var(var, value);
        }
    }
}
