use miette::MietteDiagnostic;

use crate::ast::{Expression, ExpressionValue, Primitive, Type, TypedExpression};
use crate::prelude::*;

mod error;
mod expression;

pub struct Typer<'ast> {
    errors: Vec<MietteDiagnostic>,
    expression: Expression<'ast>,
}

impl<'ast> Typer<'ast> {
    pub fn new(expression: Expression<'ast>) -> Self {
        Self {
            errors: Vec::new(),
            expression,
        }
    }

    pub fn type_check(&mut self) -> Result<TypedExpression<'ast>> {
        let expression = self.type_check_expression(&self.expression.clone())?;

        if self.errors.is_empty() {
            Ok(expression)
        } else {
            Err(self.errors.clone())
        }
    }

    fn report_error(&mut self, error: MietteDiagnostic) {
        self.errors.push(error);
    }

    fn type_check_expression(
        &mut self,
        expression: &Expression<'ast>,
    ) -> Result<TypedExpression<'ast>> {
        match &expression.value {
            ExpressionValue::Primitive(primitive) => match primitive {
                Primitive::Integer(_) => Ok(expression.with_type(Type::integer(&expression.span))),
                Primitive::Decimal(_) => Ok(expression.with_type(Type::decimal(&expression.span))),
                Primitive::String(_) => Ok(expression.with_type(Type::string(&expression.span))),
                Primitive::Character(_) => {
                    Ok(expression.with_type(Type::character(&expression.span)))
                }
                Primitive::Boolean(_) => Ok(expression.with_type(Type::boolean(&expression.span))),
                Primitive::Unit => Ok(expression.with_type(Type::unit(&expression.span))),
                Primitive::Identifier(value) => todo!(),
            },
            ExpressionValue::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.type_check_expression(&left)?;
                let right = self.type_check_expression(&right)?;

                if left.ty != right.ty {
                    self.report_error(error::new_mismatched_types(left.ty.clone(), right.ty));
                }

                Ok(expression.with_type(left.ty.clone()))
            }
        }
    }
}
