use crate::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDeclarationStatement {
    pub identifier: Identifier,
    pub type_: Type,
}

pub fn parse(parser: &mut Parser) -> Result<Statement> {
    let token = parser.expect(TokenKind::Type, "expected a type declaration")?;

    let identifier = parser.expect_identifier()?;

    parser.expect(TokenKind::Equal, "expected a type")?;

    let value = parser.parse_type(BindingPower::Assignment)?;

    let span = token.span + identifier.span + value.span;

    Ok(StatementValue::TypeDeclaration(TypeDeclarationStatement {
        identifier,
        type_: value,
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    statement: &Statement,
    env: &mut TypeEnvironment,
) -> TypedStatement {
    let declaration = match &statement.value {
        StatementValue::TypeDeclaration(declaration) => declaration,
        _ => unreachable!(),
    };

    env.declare_type(&declaration.identifier, &declaration.type_);

    TypedStatement {
        value: StatementValue::TypeDeclaration(TypeDeclarationStatement {
            identifier: declaration.identifier.clone(),
            type_: declaration.type_.clone(),
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
}
