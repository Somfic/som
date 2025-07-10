use crate::{expressions, prelude::*};

#[derive(Debug, Clone, PartialEq)]
pub struct ExternDeclarationStatement {
    pub identifier: Identifier,
    pub signature: ExternFunctionSignature,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternFunctionSignature {
    pub parameters: Vec<Parameter>,
    pub return_type: Type,
    pub span: Span,
}

pub fn parse(parser: &mut Parser) -> Result<Statement> {
    let token = parser.expect(TokenKind::Extern, "expected an extern declaration")?;

    let identifier = parser.expect_identifier()?;

    parser.expect(TokenKind::Equal, "expected a function signature")?;
    parser.expect(TokenKind::Function, "expected a function signature")?;

    let (parameters, parameters_span) = expressions::function::parse_parameters(parser)?;

    parser.expect(TokenKind::Arrow, "expected a return type")?;

    let return_type = parser.parse_type(BindingPower::None)?;

    let span = token.span + parameters_span;

    Ok(
        StatementValue::ExternDeclaration(ExternDeclarationStatement {
            identifier,
            signature: ExternFunctionSignature {
                parameters,
                return_type,
                span: parameters_span,
            },
        })
        .with_span(span),
    )
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    statement: &Statement,
    env: &mut crate::type_checker::Environment,
) -> TypedStatement {
    let extern_declaration = match &statement.value {
        StatementValue::ExternDeclaration(extern_declaration) => extern_declaration,
        _ => unreachable!(),
    };

    let type_ = TypeValue::Function(FunctionType {
        parameters: extern_declaration.signature.parameters.clone(),
        return_type: Box::new(extern_declaration.signature.return_type.clone()),
        span: extern_declaration.signature.span,
    })
    .with_span(extern_declaration.identifier.span + extern_declaration.signature.span);

    env.set(&extern_declaration.identifier, &type_);

    TypedStatement {
        value: StatementValue::ExternDeclaration(extern_declaration.clone()),
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
