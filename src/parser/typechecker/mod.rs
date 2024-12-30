use std::{borrow::Cow, collections::HashMap, fmt::format};

use super::{
    ast::{typed, untyped, Type, TypeValue},
    expression,
};
use miette::{MietteDiagnostic, Report, Result};

pub struct TypeChecker<'de> {
    symbol: untyped::Symbol<'de>,
    errors: Vec<MietteDiagnostic>,
    environment: Environment<'de>,
}

impl<'de> TypeChecker<'de> {
    pub fn new(symbol: untyped::Symbol<'de>) -> Self {
        Self {
            symbol,
            errors: vec![],
            environment: Environment::new(None),
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
            untyped::StatementValue::Expression(expression) => {
                self.check_expression(expression);
            }
            untyped::StatementValue::Block(statements) => {
                for statement in statements {
                    self.check_statement(statement);
                }
            }
            untyped::StatementValue::Function { header, body } => {
                self.check_expression(body);
            }
            untyped::StatementValue::Return(expression) => {
                self.check_expression(expression);
            }
            untyped::StatementValue::Enum { name, variants } => {}
            untyped::StatementValue::Struct { name, fields } => {}
            untyped::StatementValue::Assignment { name, value } => {
                if let Some(expression_type) = self.check_expression(value) {
                    println!("{}: {:?}", name, expression_type);
                    self.environment.set(name.clone(), expression_type);
                }
            }
            _ => todo!("check_statement: {:?}", statement),
        };
    }

    fn check_expression(&mut self, expression: &untyped::Expression<'de>) -> Option<Type<'de>> {
        match self.type_of(expression) {
            Ok(ty) => Some(ty),
            Err(err) => {
                self.errors.push(err);
                None
            }
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
                untyped::Primitive::Identifier(name) => self
                    .environment
                    .get(name)
                    .cloned()
                    .map(|e| e.span(expression.span))
                    .ok_or_else(|| MietteDiagnostic {
                        code: None,
                        severity: None,
                        url: None,
                        labels: Some(vec![expression.label("undeclared variable")]),
                        help: Some(format!("`{}` is not declared", name)),
                        message: "undeclared variable".to_owned(),
                    }),
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

struct Environment<'de> {
    parent: Option<Box<Environment<'de>>>,
    bindings: HashMap<Cow<'de, str>, Type<'de>>,
}

impl<'de> Environment<'de> {
    fn new(parent: Option<Environment<'de>>) -> Self {
        Self {
            parent: parent.map(Box::new),
            bindings: HashMap::new(),
        }
    }

    fn set(&mut self, name: Cow<'de, str>, ty: Type<'de>) {
        self.bindings.insert(name, ty);
    }

    fn get(&self, name: &Cow<'de, str>) -> Option<&Type<'de>> {
        self.bindings
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|parent| parent.get(name)))
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
                    labels.extend(left.label(format!("{} may not be used for {}", left, operator)));
                }

                if !right.value.is_numeric() {
                    labels
                        .extend(right.label(format!("{} may not be used for {}", right, operator)));
                }

                Err(MietteDiagnostic {
                    code: None,
                    severity: None,
                    url: None,
                    labels: Some(labels),
                    help: Some(format!("only numeric types may be used for {}", operator)),
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
        let mut labels = vec![];
        labels.extend(left.label(format!("{}", left)));
        labels.extend(right.label(format!("{}", right)));

        Err(MietteDiagnostic {
            code: None,
            severity: None,
            url: None,
            labels: Some(labels),
            help: Some(format!("{} is not the same type as {}", left, right)),
            message: "type mismatch".to_owned(),
        })
    }
}
