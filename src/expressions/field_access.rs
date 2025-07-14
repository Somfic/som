use crate::{prelude::*, types::StructLayout};

#[derive(Debug, Clone)]
pub struct FieldAccessExpression<Expression> {
    pub object: Box<Expression>,
    pub field: Identifier,
}

pub fn parse(
    parser: &mut Parser,
    lhs: Expression,
    _binding_power: BindingPower,
) -> Result<Expression> {
    parser.expect(TokenKind::Dot, "expected field access")?;

    let field = parser.expect_identifier()?;

    let span = lhs.span + field.span;

    Ok(ExpressionValue::FieldAccess(FieldAccessExpression {
        object: Box::new(lhs),
        field: field.clone(),
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::FieldAccess(value) => value,
        _ => unreachable!(),
    };

    let object = type_checker.check_expression(&value.object, env);

    // Check if the object is a struct type
    if let TypeValue::Struct(struct_type) = &object.type_.value {
        // Find the field in the struct
        for field in &struct_type.fields {
            if field.identifier.name == value.field.name {
                return TypedExpression {
                    type_: field.type_.as_ref().clone().with_span(expression.span),
                    value: TypedExpressionValue::FieldAccess(FieldAccessExpression {
                        object: Box::new(object),
                        field: value.field.clone(),
                    }),
                    span: expression.span,
                };
            }
        }

        // Field not found in struct
        let field_names: Vec<String> = struct_type
            .fields
            .iter()
            .map(|f| f.identifier.name.to_string())
            .collect();

        let closest_match = closest_match(field_names, value.field.name.to_string());

        let help_message = if let Some(suggestion) = closest_match {
            format!(
                "field `{}` does not exist in the struct, did you mean `{}`?",
                value.field.name, suggestion
            )
        } else {
            format!("field `{}` does not exist in the struct", value.field.name)
        };

        type_checker.add_error(Error::TypeChecker(TypeCheckerError::UnknownField {
            help: help_message,
            field: value.field.span,
            struct_span: struct_type.span,
        }));

        TypedExpression {
            type_: TypeValue::Never.with_span(expression.span),
            value: TypedExpressionValue::FieldAccess(FieldAccessExpression {
                object: Box::new(object),
                field: value.field.clone(),
            }),
            span: expression.span,
        }
    } else {
        // Not a struct type - for now, just return Never type
        TypedExpression {
            type_: TypeValue::Never.with_span(expression.span),
            value: TypedExpressionValue::FieldAccess(FieldAccessExpression {
                object: Box::new(object),
                field: value.field.clone(),
            }),
            span: expression.span,
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
        TypedExpressionValue::FieldAccess(value) => value,
        _ => unreachable!(),
    };

    // Get the struct pointer/value
    let struct_value = compiler.compile_expression(&value.object, body, env);

    // Get the struct type to compute field offset
    if let TypeValue::Struct(struct_type) = &value.object.type_.value {
        let layout = StructLayout::new(&struct_type.fields);

        if let Some(field_layout) = layout.get_field_layout(&value.field.name) {
            let field_offset = field_layout.offset as i32;
            let field_type = field_layout.type_.value.to_ir();

            // Load the field value from the struct at the correct offset
            body.ins().load(
                field_type,
                cranelift::prelude::MemFlags::new(),
                struct_value,
                field_offset,
            )
        } else {
            // Field not found - should not happen due to type checking
            body.ins().iconst(cranelift::prelude::types::I32, 0)
        }
    } else {
        // Not a struct type - should not happen due to type checking
        body.ins().iconst(cranelift::prelude::types::I32, 0)
    }
}
