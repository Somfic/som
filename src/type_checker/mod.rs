use environment::Environment;

use crate::{expressions, prelude::*, statements};
use std::cell::RefCell;

pub mod environment;

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
        let mut env = Environment::new();

        let typed_statement = self.check_statement(statement, &mut env);

        if !self.errors.borrow().is_empty() {
            Err(self.errors.borrow().clone())
        } else {
            Ok(typed_statement)
        }
    }

    pub fn check_statement(
        &mut self,
        statement: &Statement,
        env: &mut Environment,
    ) -> TypedStatement {
        match &statement.value {
            StatementValue::Expression(expression) => TypedStatement {
                value: StatementValue::Expression(self.check_expression(expression, env)),
                span: statement.span,
            },
            StatementValue::Declaration(_) => {
                statements::declaration::type_check(self, statement, env)
            }
        }
    }

    pub fn check_expression(
        &mut self,
        expression: &Expression,
        env: &mut Environment,
    ) -> TypedExpression {
        match &expression.value {
            ExpressionValue::Primary(primary) => match primary {
                PrimaryExpression::Integer(_) => {
                    expressions::primary::integer::type_check(expression)
                }
                PrimaryExpression::Boolean(_) => {
                    expressions::primary::boolean::type_check(expression)
                }
                PrimaryExpression::Unit => expressions::primary::unit::type_check(expression),
            },
            ExpressionValue::Binary(binary) => match binary.operator {
                BinaryOperator::Add => expressions::binary::add::type_check(self, expression, env),
                BinaryOperator::Subtract => {
                    expressions::binary::subtract::type_check(self, expression, env)
                }
                BinaryOperator::Multiply => {
                    expressions::binary::multiply::type_check(self, expression, env)
                }
                BinaryOperator::Divide => {
                    expressions::binary::divide::type_check(self, expression, env)
                }
            },
            ExpressionValue::Group(_) => expressions::group::type_check(self, expression, env),
            ExpressionValue::Block(_) => expressions::block::type_check(self, expression, env),
            ExpressionValue::Identifier(_) => {
                expressions::identifier::type_check(self, expression, env)
            }
            ExpressionValue::Function(_) => {
                expressions::function::type_check(self, expression, env)
            }
            ExpressionValue::Call(_) => expressions::call::type_check(self, expression, env),
        }
    }

    pub fn expect_type(
        &self,
        actual: &Type,
        expected: &Type,
        expected_span: impl Into<Span>,
        message: impl Into<String>,
    ) -> TypeValue {
        if actual.value == expected.value {
            return actual.value.clone();
        }

        if actual.value == TypeValue::Never || expected.value == TypeValue::Never {
            return TypeValue::Never;
        }

        self.errors.borrow_mut().push(type_checker_unexpected_type(
            expected,
            actual,
            expected_span,
            message,
        ));

        TypeValue::Never
    }

    pub fn expect_same_type(&self, types: Vec<&Type>, message: impl Into<String>) -> TypeValue {
        let most_occuring_type = if types.len() <= 2 {
            None
        } else {
            types
                .iter()
                .fold(std::collections::HashMap::new(), |mut acc, ty| {
                    *acc.entry(ty.value.clone()).or_insert(0) += 1;
                    acc
                })
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map(|(kind, _)| kind)
        };

        if types.iter().any(|t| t.value == TypeValue::Never) {
            return most_occuring_type.unwrap_or(TypeValue::Never);
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

        ty.unwrap_or(&TypeValue::Never).clone()
    }

    pub fn expect_declaration(
        &self,
        identifier: &Identifier,
        env: &mut Environment,
        message: impl Into<String>,
    ) -> Type {
        let type_ = env
            .get(identifier)
            .unwrap_or(Type::new(identifier, TypeValue::Never));

        if type_.value == TypeValue::Never {
            self.errors
                .borrow_mut()
                .push(declaration_not_found(identifier, message));
        }

        type_
    }

    pub fn add_error(&self, error: Error) {
        self.errors.borrow_mut().push(error);
    }
}
