use std::fmt::format;

use super::{
    ast::{typed, untyped, Type, TypeValue},
    expression,
};
use miette::{MietteDiagnostic, Report, Result};

pub struct TypeChecker<'de> {
    symbol: untyped::Symbol<'de>,
    errors: Vec<MietteDiagnostic>,
}

impl<'de> TypeChecker<'de> {
    pub fn new(symbol: untyped::Symbol<'de>) -> Self {
        Self {
            symbol,
            errors: vec![],
        }
    }

    pub fn check(mut self) -> Vec<MietteDiagnostic> {
        match self.symbol.clone() {
            untyped::Symbol::Statement(statement) => {
                self.check_statement(&statement);
            }
            untyped::Symbol::Expression(expression) => {
                self.check_expression(&expression);
            }
        };

        self.errors
    }

    fn check_statement(&mut self, statement: &untyped::Statement<'de>) {
        match &statement.value {
            untyped::StatementValue::Expression(expression) => self.check_expression(expression),
            untyped::StatementValue::Block(statements) => {
                for statement in statements {
                    self.check_statement(statement);
                }
            }
            untyped::StatementValue::Function { header, body } => self.check_expression(body),
            untyped::StatementValue::Return(expression) => self.check_expression(expression),
            untyped::StatementValue::Enum { name, variants } => {}
            untyped::StatementValue::Struct { name, fields } => {}
            untyped::StatementValue::Assignment { name, value } => self.check_expression(value),
            _ => todo!("check_statement: {:?}", statement),
        }
    }

    fn check_expression(&mut self, expression: &untyped::Expression<'de>) {
        if let Err(err) = self.type_of(expression) {
            self.errors.push(err);
        }
    }

    fn type_of(
        &mut self,
        expression: &untyped::Expression<'de>,
    ) -> Result<Type<'de>, MietteDiagnostic> {
        match &expression.value {
            untyped::ExpressionValue::Primitive(primitive) => match primitive {
                untyped::Primitive::Integer(_) => Ok(Type::integer(expression.span)),
                untyped::Primitive::Decimal(_) => Ok(Type::decimal(expression.span)),
                untyped::Primitive::Boolean(_) => Ok(Type::boolean(expression.span)),
                untyped::Primitive::String(_) => Ok(Type::string(expression.span)),
                untyped::Primitive::Identifier(name) => {
                    Ok(Type::symbol(expression.span, name.clone()))
                }
                untyped::Primitive::Character(_) => Ok(Type::character(expression.span)),
                untyped::Primitive::Unit => Ok(Type::unit(expression.span)),
            },
            untyped::ExpressionValue::Group(expression) => self.type_of(expression),
            untyped::ExpressionValue::Block {
                statements,
                return_value,
            } => {
                for statement in statements {
                    self.check_statement(statement);
                }

                self.type_of(return_value)
            }
            untyped::ExpressionValue::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.type_of(left)?;
                let right = self.type_of(right)?;
                expect_allowed_binary_operation(&left, &right, operator)?;
                expect_match(&left, &right)?;
                Ok(left.clone())
            }
            untyped::ExpressionValue::Unary { operator, operand } => self.type_of(operand),
            _ => todo!("type_of: {:?}", expression),
        }
    }
}

fn expect_allowed_binary_operation<'de>(
    left: &Type<'de>,
    right: &Type<'de>,
    operator: &untyped::BinaryOperator,
) -> Result<(), MietteDiagnostic> {
    match operator {
        untyped::BinaryOperator::Add
        | untyped::BinaryOperator::Subtract
        | untyped::BinaryOperator::Multiply
        | untyped::BinaryOperator::Divide => {
            if left.value.is_numeric() && right.value.is_numeric() {
                Ok(())
            } else {
                let mut labels = vec![];

                if !left.value.is_numeric() {
                    labels.push(left.label(format!("{}", left)));
                }

                if !right.value.is_numeric() {
                    labels.push(right.label(format!("{}", right)));
                }

                Err(MietteDiagnostic {
                    code: None,
                    severity: None,
                    url: None,
                    labels: Some(labels),
                    help: Some(format!("only numeric types can be used for {}", operator)),
                    message: format!("expected numeric types for {}", operator),
                })
            }
        }
        _ => todo!("expect_allowed_binary_operation: {:?}", operator),
    }
}

fn expect_match<'de>(left: &Type<'de>, right: &Type<'de>) -> Result<(), MietteDiagnostic> {
    if left.value.matches(&right.value) {
        Ok(())
    } else {
        Err(MietteDiagnostic {
            code: None,
            severity: None,
            url: None,
            labels: Some(vec![
                left.label(format!("{}", left)),
                right.label(format!("{}", right)),
            ]),
            help: Some(format!("{} is not the same type as {}", left, right)),
            message: "type mismatch".to_owned(),
        })
    }
}
