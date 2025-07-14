use crate::{expressions, prelude::*, compiler::external};

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

    // Validate that the extern function is defined in the compiler
    let available_functions = external::get_available_extern_functions();
    let function_name = &extern_declaration.identifier.name;
    
    if !available_functions.iter().any(|f| f == function_name.as_ref()) {
        let closest = closest_match(available_functions.clone(), function_name.to_string());
        
        let help_message = if let Some(suggestion) = closest {
            // Check if the suggestion is reasonable (contains some similar characters)
            if function_name.chars().any(|c| suggestion.contains(c)) && 
               (function_name.len() as i32 - suggestion.len() as i32).abs() <= 3 {
                format!("Function '{}' is not defined in the compiler. Did you mean '{}'?", function_name, suggestion)
            } else {
                format!("Function '{}' is not defined in the compiler. Available functions: {}", function_name, available_functions.join(", "))
            }
        } else {
            format!("Function '{}' is not defined in the compiler. Available functions: {}", function_name, available_functions.join(", "))
        };
        
        type_checker.add_error(Error::TypeChecker(TypeCheckerError::UnknownExternFunction {
            function_span: extern_declaration.identifier.span,
            help: help_message,
        }));
    }

    let type_ = TypeValue::Function(FunctionType {
        parameters: extern_declaration.signature.parameters.clone(),
        return_type: Box::new(extern_declaration.signature.return_type.clone()),
        span: extern_declaration.signature.span,
    })
    .with_span(extern_declaration.identifier.span + extern_declaration.signature.span);

    env.declare(&extern_declaration.identifier, &type_);

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
