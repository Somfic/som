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

    pub fn type_check(&mut self) -> ParserResult<TypedExpression<'ast>> {
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
    ) -> ParserResult<TypedExpression<'ast>> {
        match &expression.value {
            ExpressionValue::Primitive(primitive) => match primitive {
                Primitive::Integer(_) => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Type::integer(&expression.span),
                    span: expression.span,
                }),
                Primitive::Decimal(_) => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Type::decimal(&expression.span),
                    span: expression.span,
                }),
                Primitive::String(_) => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Type::string(&expression.span),
                    span: expression.span,
                }),
                Primitive::Character(_) => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Type::character(&expression.span),
                    span: expression.span,
                }),
                Primitive::Boolean(_) => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Type::boolean(&expression.span),
                    span: expression.span,
                }),
                Primitive::Unit => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Type::unit(&expression.span),
                    span: expression.span,
                }),
                Primitive::Identifier(value) => todo!(),
            },
            ExpressionValue::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.type_check_expression(left)?;
                let right = self.type_check_expression(right)?;
                let left_ty = left.ty.clone();

                if left_ty != right.ty {
                    self.report_error(error::new_mismatched_types(
                        "expected types to binary operation to match",
                        &left_ty,
                        &right.ty,
                    ));
                }

                Ok(TypedExpression {
                    value: ExpressionValue::Binary {
                        operator: operator.clone(),
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    ty: left_ty,
                    span: expression.span,
                })
            }
            ExpressionValue::Group(expression) => self.type_check_expression(expression),
        }
    }
}
