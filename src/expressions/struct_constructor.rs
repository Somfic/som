use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct StructConstructorExpression<Expression> {
    pub type_identifier: Identifier,
    pub type_: Type,
    pub fields: Vec<Field<Expression>>,
}

#[derive(Debug, Clone)]
struct Field<Expression> {
    pub span: Span,
    pub identifier: Identifier,
    pub value: Box<Expression>,
}

pub fn parse(
    parser: &mut Parser,
    lhs: Expression,
    binding_power: BindingPower,
) -> Result<Expression> {
    let type_identifier = match lhs.value {
        ExpressionValue::Identifier(identifier) => identifier,
        _ => return Err(parser_expected_identifier(lhs.span)),
    };

    parser.expect(TokenKind::CurlyOpen, "expected a struct constructor")?;

    let mut fields = vec![];

    loop {
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::CurlyClose)
        }) {
            break;
        }

        if !fields.is_empty() {
            parser.expect(TokenKind::Comma, "expected a comma between fields")?;
        }

        let identifier = parser.expect_identifier()?;

        parser.expect(
            TokenKind::Colon,
            format!("expected a value for `{}`", identifier.name),
        )?;

        let value = parser.parse_expression(BindingPower::None)?;

        fields.push(Field {
            span: identifier.span + value.span,
            identifier,
            value: Box::new(value),
        });
    }

    let end = parser.expect(
        TokenKind::CurlyClose,
        "expected closing brace for struct constructor",
    )?;

    let span = type_identifier.span + end.span;

    Ok(
        ExpressionValue::StructConstructor(StructConstructorExpression {
            type_: TypeValue::Never.with_span(type_identifier.span), // this will be filled in with the type check pass
            type_identifier,
            fields,
        })
        .with_span(span),
    )
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::StructConstructor(value) => value,
        _ => unreachable!(),
    };

    let type_ = env.get_type(&value.type_identifier).unwrap();

    let fields = value
        .fields
        .iter()
        .map(|field| {
            let value = type_checker.check_expression(&field.value, env);
            Field {
                span: field.span,
                identifier: field.identifier.clone(),
                value: Box::new(value),
            }
        })
        .collect::<Vec<_>>();

    // Check if all fields are present

    TypedExpression {
        type_: type_.clone().with_span(expression.span),
        value: TypedExpressionValue::StructConstructor(StructConstructorExpression {
            type_identifier: value.type_identifier.clone(),
            type_,
            fields,
        }),
        span: expression.span,
    }
}

fn check_fields(
    type_checker: &mut TypeChecker,
    arguments: &[TypedExpression],
    fields: &[Field<TypedExpression>],
    missing_field_offset: usize,
    function: &FunctionType,
) {
    if arguments.len() < fields.len() {
        for field in &fields[arguments.len()..] {
            type_checker.add_error(Error::TypeChecker(TypeCheckerError::MissingParameter {
                help: format!(
                    "supply a value for `{}` ({})",
                    field.identifier, field.type_
                ),
                argument: (missing_field_offset, 0),
                parameter: field.clone(),
            }));
        }
    }

    if fields.len() < arguments.len() {
        for argument in &arguments[fields.len()..] {
            type_checker.add_error(Error::TypeChecker(TypeCheckerError::UnexpectedArgument {
                help: "remove this argument or add a parameter to the function signature"
                    .to_string(),
                argument: argument.clone(),
                function: function.clone(),
            }));
        }
    }

    for (argument, parameter) in arguments.iter().zip(fields) {
        let argument_type = &argument.type_;
        let parameter_type = &parameter.type_;

        type_checker.expect_type(
            argument_type,
            parameter_type,
            parameter,
            format!("for parameter `{}`", parameter.identifier),
        );
    }
}
