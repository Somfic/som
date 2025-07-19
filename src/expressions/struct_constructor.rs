use crate::{prelude::*, types::struct_::Field, types::StructLayout};

#[derive(Debug, Clone)]
pub struct StructConstructorExpression<Expression> {
    pub type_identifier: Identifier,
    pub type_: Type,
    pub arguments: Vec<FieldArgument<Expression>>,
}

#[derive(Debug, Clone)]
pub struct FieldArgument<Expression> {
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

    let mut arguments = vec![];

    loop {
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::CurlyClose)
        }) {
            break;
        }

        if !arguments.is_empty() {
            parser.expect(
                TokenKind::Comma,
                "expected a comma between struct arguments",
            )?;
        }

        let identifier = parser.expect_identifier()?;

        parser.expect(
            TokenKind::Colon,
            format!("expected a value for `{}`", identifier.name),
        )?;

        let value = parser.parse_expression(BindingPower::None)?;

        arguments.push(FieldArgument {
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
            arguments,
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

    type_checker.expect_struct_type(&type_, "expected a struct for struct constructor");

    let arguments = value
        .arguments
        .iter()
        .map(|field| {
            let value = type_checker.check_expression(&field.value, env);
            FieldArgument {
                span: field.span,
                identifier: field.identifier.clone(),
                value: Box::new(value),
            }
        })
        .collect::<Vec<_>>();

    // Enable field validation
    if let TypeValue::Struct(struct_) = &type_.value {
        let fields = struct_.fields.clone();
        check_fields(
            type_checker,
            &arguments,
            &fields,
            0,
            struct_,
            expression.span,
        );
    }

    TypedExpression {
        type_: type_.clone().with_span(expression.span),
        value: TypedExpressionValue::StructConstructor(StructConstructorExpression {
            type_identifier: value.type_identifier.clone(),
            type_,
            arguments,
        }),
        span: expression.span,
    }
}

fn check_fields(
    type_checker: &mut TypeChecker,
    arguments: &[FieldArgument<TypedExpression>],
    fields: &[Field],
    _missing_field_offset: usize,
    _struct_: &StructType,
    constructor_span: Span,
) {
    // Check that all provided fields exist in the struct
    for argument in arguments {
        let field_found = fields
            .iter()
            .any(|field| field.identifier.name == argument.identifier.name);
        if !field_found {
            let field_names: Vec<String> = fields
                .iter()
                .map(|f| f.identifier.name.to_string())
                .collect();

            let closest_match = closest_match(field_names, argument.identifier.name.to_string());

            let help_message = if let Some(suggestion) = closest_match {
                format!(
                    "field `{}` not found in struct, did you mean `{}`?",
                    argument.identifier.name, suggestion
                )
            } else {
                format!("field `{}` not found in struct", argument.identifier.name)
            };

            type_checker.add_error(Error::TypeChecker(TypeCheckerError::UnknownField {
                help: help_message,
                field: argument.identifier.span,
                struct_span: _struct_.span,
            }));
        }
    }

    // Check that all required fields are provided
    for field in fields {
        let arg_found = arguments
            .iter()
            .any(|arg| arg.identifier.name == field.identifier.name);
        if !arg_found {
            type_checker.add_error(Error::TypeChecker(TypeCheckerError::MissingRequiredField {
                help: format!(
                    "missing field `{}` in struct constructor",
                    field.identifier.name
                ),
                field: field.identifier.span,
                constructor: constructor_span,
            }));
        }
    }

    // Check that field types match
    for argument in arguments {
        if let Some(field) = fields
            .iter()
            .find(|f| f.identifier.name == argument.identifier.name)
        {
            type_checker.expect_type(
                &argument.value.type_,
                &field.type_,
                field.identifier.span,
                format!("for field `{}`", field.identifier.name),
            );
        }
    }
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::StructConstructor(value) => value,
        _ => unreachable!(),
    };

    if let TypeValue::Struct(struct_type) = &value.type_.value {
        let layout = StructLayout::new(&struct_type.fields);

        // For now, we'll use a simplified approach where we allocate memory on the stack
        // In a more complete implementation, you might want heap allocation
        let struct_size = layout.total_size as i32;

        // Allocate stack space for the struct
        let stack_slot = body.create_sized_stack_slot(cranelift::prelude::StackSlotData::new(
            cranelift::prelude::StackSlotKind::ExplicitSlot,
            struct_size as u32,
            0, // No additional alignment
        ));

        // Get the address of the stack slot
        let struct_ptr = body
            .ins()
            .stack_addr(cranelift::prelude::types::I64, stack_slot, 0);

        // Store each field at its correct offset
        for argument in &value.arguments {
            if let Some(field_layout) = layout.get_field_layout(&argument.identifier.name) {
                let field_value = compiler.compile_expression(&argument.value, body, env);
                let field_offset = field_layout.offset as i32;

                // Store the field value at the correct offset
                body.ins().store(
                    cranelift::prelude::MemFlags::new(),
                    field_value,
                    struct_ptr,
                    field_offset,
                );
            }
        }

        // Return the pointer to the struct
        struct_ptr
    } else {
        unreachable!("Expected struct type")
    }
}
