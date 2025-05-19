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
            span: statement.span.clone(),
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
            },
        }
    }

    pub fn expect_same_type(&self, types: Vec<&Type>, message: &str) {
        let mut ty = types.first().map(|t| &t.kind).unwrap_or(&TypeKind::Never);

        for type_ in types.iter().skip(1) {
            if type_.kind != *ty {
                ty = &TypeKind::Never;
                break;
            } else {
                ty = &type_.kind;
            }
        }

        if ty == &TypeKind::Never {
            self.errors
                .borrow_mut()
                .push(type_checker_type_mismatch(types, message));
        }
    }
}
