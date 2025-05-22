use crate::{prelude::*, type_checker::environment::Environment};

#[derive(Debug, Clone, PartialEq)]
pub struct DeclarationStatement<Expression> {
    pub identifier: Identifier,
    pub explicit_type: Option<Type>,
    pub value: Box<Expression>,
}

pub fn parse(parser: &mut Parser) -> Result<Statement> {
    let token = parser.expect(TokenKind::Let, "expected a variable declaration")?;

    let identifier = parser.parse_identifier()?;

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
    env: &mut Environment,
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
