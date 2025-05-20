use crate::{expressions, prelude::*};
use std::cell::RefCell;

pub struct TypeChecker {
    errors: RefCell<Vec<Error>>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            errors: RefCell::new(Vec::new()),
        }
    }

    pub fn check(&mut self, statement: &Statement) -> Results<TypedStatement> {
        let typed_statement = self.check_statement(statement);

        if !self.errors.borrow().is_empty() {
            Err(self.errors.borrow().clone())
        } else {
            Ok(typed_statement)
        }
    }

    pub fn check_statement(&mut self, statement: &Statement) -> TypedStatement {
        let value = match &statement.value {
            StatementValue::Expression(expression) => {
                StatementValue::Expression(self.check_expression(expression))
            }
        };

        TypedStatement {
            value,
            span: statement.span,
        }
    }

    pub fn check_expression(&mut self, expression: &Expression) -> TypedExpression {
        match &expression.value {
            ExpressionValue::Primary(primary) => match primary {
                PrimaryExpression::Integer(_) => {
                    expressions::primary::integer::type_check(expression)
                }
                PrimaryExpression::Boolean(_) => {
                    expressions::primary::boolean::type_check(expression)
                }
            },
            ExpressionValue::Binary(binary) => match binary.operator {
                BinaryOperator::Add => expressions::binary::add::type_check(self, expression),
                BinaryOperator::Subtract => {
                    expressions::binary::subtract::type_check(self, expression)
                }
                BinaryOperator::Multiply => {
                    expressions::binary::multiply::type_check(self, expression)
                }
                BinaryOperator::Divide => expressions::binary::divide::type_check(self, expression),
            },
            ExpressionValue::Group(_) => expressions::group::type_check(self, expression),
        }
    }

    pub fn expect_same_type(&self, types: Vec<&Type>, message: &str) -> TypeValue {
        if types.iter().any(|t| t.value == TypeValue::Never) {
            return TypeValue::Never;
        }

        let mut ty = types.first().map(|t| Some(&t.value)).unwrap_or(None);

        for type_ in types.iter().skip(1) {
            if ty.is_none() {
                break;
            }

            if Some(&type_.value) != ty {
                ty = None;
                break;
            } else {
                ty = Some(&type_.value);
            }
        }

        if ty.is_none() {
            self.errors
                .borrow_mut()
                .push(type_checker_type_mismatch(types, message));
        }

        *ty.unwrap_or(&TypeValue::Never)
    }
}
