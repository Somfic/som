use crate::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportStatement {
    pub path: String,
    pub path_span: Span,
}

pub fn parse(parser: &mut Parser) -> Result<Statement> {
    let token = parser.expect(TokenKind::Use, "expected an import statement")?;

    let string_token = parser.expect(TokenKind::String, "expected a file path")?;

    let path = match &string_token.value {
        TokenValue::String(s) => s.to_string(),
        _ => unreachable!(),
    };

    let span = token.span + string_token.span;

    Ok(StatementValue::Import(ImportStatement {
        path,
        path_span: string_token.span,
    }).with_span(span))
}

pub fn type_check(
    _type_checker: &mut TypeChecker,
    statement: &GenericStatement<Expression>,
    _env: &mut TypeEnvironment,
) -> TypedStatement {
    // Import statements are handled by the module loader during the compilation pipeline
    // We just need to return a typed version of the statement for now
    let import_stmt = match &statement.value {
        StatementValue::Import(import) => import,
        _ => unreachable!(),
    };

    TypedStatement {
        value: StatementValue::Import(import_stmt.clone()),
        span: statement.span,
    }
}

pub fn compile(
    _compiler: &mut Compiler,
    _statement: &TypedStatement,
    _body: &mut FunctionBuilder<'_>,
    _env: &mut CompileEnvironment,
) {
    // Import statements don't generate any code
    // The actual imported functions are compiled separately by the module loader
}
