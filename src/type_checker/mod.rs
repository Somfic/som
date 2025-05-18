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

    pub fn check(&mut self, statement: &Statement) {
        match &statement.value {
            StatementValue::Expression(expression) => {
                let result = self.check_expression(expression);
                if let Err(error) = result {
                    self.errors.borrow_mut().push(error);
                }
            }
        };

        if !self.errors.borrow().is_empty() {
            panic!(
                "Type checking failed with errors: {:?}",
                self.errors.borrow()
            );
        }
    }

    pub fn check_expression(&mut self, expression: &Expression) -> Result<TypedExpression> {
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

    pub fn expect_same_type(&self, left: &Type, right: &Type, message: &str) -> Result<()> {
        if !left.equals(right) {
            Err(Error::TypeChecker(TypeCheckerError::TypeMismatch {
                left: left.into(),
                right: right.into(),
                help: message.to_string(),
            }))
        } else {
            Ok(())
        }
    }
}
