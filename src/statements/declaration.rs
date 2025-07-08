use crate::{expressions, prelude::*};

#[derive(Debug, Clone, PartialEq)]
pub struct DeclarationStatement<Expression> {
    pub identifier: Identifier,
    pub explicit_type: Option<Type>,
    pub value: Box<Expression>,
}

pub fn parse(parser: &mut Parser) -> Result<Statement> {
    let token = parser.expect(TokenKind::Let, "expected a variable declaration")?;

    let identifier = parser.expect_identifier()?;

    let explicit_type = None;

    parser.expect(TokenKind::Equal, "expected a value")?;

    let value = parser.parse_expression(BindingPower::Assignment)?;

    let span = token.span + identifier.span + value.span;

    Ok(StatementValue::Declaration(DeclarationStatement {
        identifier,
        explicit_type,
        value: Box::new(value),
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    statement: &Statement,
    env: &mut crate::type_checker::Environment,
) -> TypedStatement {
    let declaration = match &statement.value {
        StatementValue::Declaration(declaration) => declaration,
        _ => unreachable!(),
    };

    let value = type_checker.check_expression(&declaration.value, env);

    env.set(&declaration.identifier, &value.type_);

    TypedStatement {
        value: StatementValue::Declaration(DeclarationStatement {
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
        StatementValue::Declaration(declaration) => declaration,
        _ => unreachable!(),
    };

    match &declaration.value.value {
        TypedExpressionValue::Function(_) => {
            let func_id = expressions::function::compile(compiler, &declaration.value, env);
            env.declare_function(&declaration.identifier, func_id);
        }
        _ => {
            compiler.compile_expression(&declaration.value, body, env);
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
