use crate::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportStatement {
    pub identifier: Identifier,
}

pub fn parse(parser: &mut Parser) -> Result<Statement> {
    let token = parser.expect(TokenKind::Use, "expected an import statement")?;

    let identifier = parser.expect_identifier()?;

    let span = token.span + identifier.span;

    Ok(StatementValue::Import(ImportStatement { identifier }).with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    statement: &GenericStatement<Expression>,
    env: &mut TypeEnvironment,
) -> TypedStatement {
    todo!()
}

pub fn compile(
    compiler: &mut Compiler,
    statement: &TypedStatement,
    body: &mut FunctionBuilder<'_>,
    env: &mut CompileEnvironment,
) {
    todo!()
}
