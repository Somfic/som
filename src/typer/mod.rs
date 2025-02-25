use std::borrow::Cow;
use std::env;

use environment::Environment;
use miette::MietteDiagnostic;

use crate::ast::{
    Expression, ExpressionValue, Primitive, Statement, StatementValue, Type, TypeValue,
    TypedExpression, TypedStatement,
};
use crate::prelude::*;

mod environment;
mod error;

pub struct Typer {
    errors: Vec<MietteDiagnostic>,
}

impl Typer {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn type_check<'ast>(
        &mut self,
        statements: Vec<Statement<'ast>>,
    ) -> ParserResult<Vec<TypedStatement<'ast>>> {
        let mut environment = Environment::new();

        let mut typed_statements: Vec<TypedStatement<'ast>> = Vec::new();

        for statement in &statements {
            let statement = self.type_check_statement(statement, &mut environment)?;
            typed_statements.push(statement);
        }

        if self.errors.is_empty() {
            Ok(typed_statements)
        } else {
            Err(self.errors.clone())
        }
    }

    fn report_error(&mut self, error: MietteDiagnostic) {
        self.errors.push(error);
    }

    fn type_check_expression<'ast>(
        &mut self,
        expression: &Expression<'ast>,
        environment: &mut Environment<'_, 'ast>,
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
                Primitive::Identifier(value) => match environment.lookup(value) {
                    Some(ty) => Ok(TypedExpression {
                        value: ExpressionValue::Primitive(primitive.clone()),
                        ty: ty.clone().span(expression.span),
                        span: expression.span,
                    }),
                    None => {
                        self.report_error(error::undefined_identifier(
                            format!("the identifier {value} is not defined"),
                            value,
                            expression.span,
                        ));
                        Ok(TypedExpression {
                            value: ExpressionValue::Primitive(primitive.clone()),
                            ty: Type::unknown(&expression.span),
                            span: expression.span,
                        })
                    }
                },
            },
            ExpressionValue::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.type_check_expression(left, environment)?;
                let right = self.type_check_expression(right, environment)?;
                let left_ty = left.ty.clone();

                if left_ty != right.ty {
                    self.report_error(error::new_mismatched_types(
                        format!("expected the types between {operator} to match"),
                        &left_ty,
                        &right.ty,
                        format!("{left_ty} and {} do not match", right.ty),
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
            ExpressionValue::Group(expression) => {
                self.type_check_expression(expression, environment)
            }
            ExpressionValue::Unary { operator, operand } => match operator {
                crate::ast::UnaryOperator::Negate => todo!(),
                crate::ast::UnaryOperator::Negative => Ok(TypedExpression {
                    value: ExpressionValue::Unary {
                        operator: operator.clone(),
                        operand: Box::new(self.type_check_expression(operand, environment)?),
                    },
                    ty: Type::integer(&expression.span),
                    span: expression.span,
                }),
            },
            ExpressionValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let condition = self.type_check_expression(condition, environment)?;
                let truthy = self.type_check_expression(truthy, environment)?;
                let falsy = self.type_check_expression(falsy, environment)?;
                let truthy_ty = truthy.ty.clone();

                if condition.ty.value != TypeValue::Boolean {
                    self.report_error(error::new_mismatched_types(
                        "expected the condition to be a boolean",
                        &condition.ty,
                        &Type::boolean(&condition.span),
                        format!("{} is not a boolean", condition.ty),
                    ));
                }

                if truthy_ty != falsy.ty {
                    self.report_error(error::new_mismatched_types(
                        "expected the types of the truthy and falsy branches to match",
                        &truthy.ty,
                        &falsy.ty,
                        format!("{} and {} do not match", truthy.ty, falsy.ty),
                    ));
                }

                Ok(TypedExpression {
                    ty: truthy_ty.span(condition.span),
                    value: ExpressionValue::Conditional {
                        condition: Box::new(condition),
                        truthy: Box::new(truthy),
                        falsy: Box::new(falsy),
                    },
                    span: expression.span,
                })
            }
            ExpressionValue::Block { statements, result } => {
                let statements = statements
                    .iter()
                    .map(|statement| self.type_check_statement(statement, environment))
                    .collect::<ParserResult<Vec<_>>>()?;

                let result = self.type_check_expression(result, environment)?;
                let result_ty = result.ty.clone();

                Ok(TypedExpression {
                    value: ExpressionValue::Block {
                        statements,
                        result: Box::new(result),
                    },
                    ty: result_ty,
                    span: expression.span,
                })
            }
        }
    }

    fn type_check_statement<'ast>(
        &mut self,
        statement: &Statement<'ast>,
        environment: &mut Environment<'_, 'ast>,
    ) -> ParserResult<TypedStatement<'ast>> {
        match &statement.value {
            StatementValue::Block(statements) => {
                let environment = &mut environment.block();

                let statements = statements
                    .iter()
                    .map(|statement| self.type_check_statement(statement, environment))
                    .collect::<ParserResult<Vec<_>>>()?;

                Ok(TypedStatement {
                    value: StatementValue::Block(statements),
                    span: statement.span,
                })
            }
            StatementValue::Expression(expression) => {
                let expression = self.type_check_expression(expression, environment)?;
                Ok(TypedStatement {
                    value: StatementValue::Expression(expression),
                    span: statement.span,
                })
            }
            StatementValue::Declaration(name, expression) => {
                let expression = self.type_check_expression(expression, environment)?;

                environment.declare(name.clone(), expression.ty.clone());

                Ok(TypedStatement {
                    value: StatementValue::Declaration(name.clone(), expression),
                    span: statement.span,
                })
            }
        }
    }
}
