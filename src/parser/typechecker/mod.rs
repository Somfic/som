use std::{borrow::Cow, collections::HashMap};

use super::{
    ast::{typed, untyped, Type, TypeValue},
    expression,
};
use miette::{MietteDiagnostic, Report, Result};

pub struct TypeChecker<'ast> {
    symbol: untyped::Symbol<'ast>,
    errors: Vec<MietteDiagnostic>,
}

impl<'ast> TypeChecker<'ast> {
    pub fn new(symbol: untyped::Symbol<'ast>) -> Self {
        Self {
            symbol,
            errors: vec![],
        }
    }

    pub fn check(mut self) -> Vec<MietteDiagnostic> {
        // Initially, there is no parent environment
        let mut environment = Environment::new(None);

        match self.symbol.clone() {
            untyped::Symbol::Statement(stmt) => {
                self.check_statement(&stmt, &mut environment);
            }
            untyped::Symbol::Expression(expr) => {
                self.check_expression(&expr, &environment);
            }
        };

        self.errors
    }

    fn check_statement<'env>(
        &mut self,
        statement: &untyped::Statement<'ast>,
        environment: &mut Environment<'env, 'ast>,
    ) {
        match &statement.value {
            untyped::StatementValue::Expression(expr) => {
                self.check_expression(expr, environment);
            }
            untyped::StatementValue::Block(statements) => {
                // Create a new child environment
                let mut environment = Environment::new(Some(environment));

                for stmt in statements {
                    self.check_statement(stmt, &mut environment);
                }
            }
            untyped::StatementValue::Function { header, body } => {
                self.check_expression(body, environment);
            }
            untyped::StatementValue::Return(expr) => {
                self.check_expression(expr, environment);
            }
            untyped::StatementValue::Enum { name, variants } => {
                // Not implemented yet
            }
            untyped::StatementValue::Struct { name, fields } => {
                // Not implemented yet
            }
            untyped::StatementValue::Assignment { name, value } => {
                if let Some(expression_type) = self.check_expression(value, environment) {
                    println!("{}: {:?}", name, expression_type);
                    environment.set(name.clone(), expression_type);
                }
            }
            _ => todo!("check_statement: {:?}", statement),
        };
    }

    fn check_expression<'env>(
        &mut self,
        expression: &untyped::Expression<'ast>,
        environment: &'env Environment<'env, 'ast>,
    ) -> Option<Type<'ast>> {
        match self.type_of(expression, environment) {
            Ok(ty) => Some(ty),
            Err(err) => {
                self.errors.extend(err);
                None
            }
        }
    }

    fn type_of<'env>(
        &mut self,
        expression: &untyped::Expression<'ast>,
        environment: &Environment<'env, 'ast>,
    ) -> Result<Type<'ast>, Vec<MietteDiagnostic>> {
        match &expression.value {
            untyped::ExpressionValue::Primitive(primitive) => match primitive {
                untyped::Primitive::Integer(_) => Ok(Type::integer(expression.span)),
                untyped::Primitive::Decimal(_) => Ok(Type::decimal(expression.span)),
                untyped::Primitive::Boolean(_) => Ok(Type::boolean(expression.span)),
                untyped::Primitive::String(_) => Ok(Type::string(expression.span)),
                untyped::Primitive::Identifier(name) => environment
                    .get(name)
                    .cloned()
                    .map(|ty| ty.span(expression.span))
                    .ok_or_else(|| {
                        vec![MietteDiagnostic {
                            code: None,
                            severity: None,
                            url: None,
                            labels: Some(vec![expression.label("undeclared variable")]),
                            help: Some(format!("`{}` is not declared", name)),
                            message: "undeclared variable".to_owned(),
                        }]
                    }),
                untyped::Primitive::Character(_) => Ok(Type::character(expression.span)),
                untyped::Primitive::Unit => Ok(Type::unit(expression.span)),
            },
            untyped::ExpressionValue::Group(expr) => self.type_of(expr, environment),
            untyped::ExpressionValue::Block {
                statements,
                return_value,
            } => {
                // Child environment
                let mut environment = Environment::new(Some(environment));

                for stmt in statements {
                    self.check_statement(stmt, &mut environment);
                }

                self.type_of(return_value, &environment)
            }
            untyped::ExpressionValue::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.type_of(left, environment)?;
                let right = self.type_of(right, environment)?;
                self.expect_match(&left, &right);
                self.expect_allowed_binary_operation(&left, &right, operator);
                Ok(left)
            }
            untyped::ExpressionValue::Unary { operator, operand } => {
                self.type_of(operand, environment)
            }
            untyped::ExpressionValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let condition = self.type_of(condition, environment)?;
                let truthy = self.type_of(truthy, environment)?;
                let falsy = self.type_of(falsy, environment)?;

                self.expect_match(&truthy, &falsy);

                Ok(truthy)
            }
            _ => todo!("type_of: {:?}", expression),
        }
    }

    fn expect_allowed_binary_operation<'a>(
        &mut self,
        left: &Type<'a>,
        right: &Type<'a>,
        operator: &untyped::BinaryOperator,
    ) {
        match operator {
            untyped::BinaryOperator::Add
            | untyped::BinaryOperator::Subtract
            | untyped::BinaryOperator::Multiply
            | untyped::BinaryOperator::Divide
            | untyped::BinaryOperator::Modulo
            | untyped::BinaryOperator::LessThan
            | untyped::BinaryOperator::LessThanOrEqual
            | untyped::BinaryOperator::GreaterThan
            | untyped::BinaryOperator::GreaterThanOrEqual => {
                if !left.value.is_numeric() || !right.value.is_numeric() {
                    let mut labels = vec![];
                    if !left.value.is_numeric() {
                        labels.extend(
                            left.label(format!("{} may not be used for {}", left, operator)),
                        );
                    }
                    if !right.value.is_numeric() {
                        labels.extend(
                            right.label(format!("{} may not be used for {}", right, operator)),
                        );
                    }

                    self.errors.push(MietteDiagnostic {
                        code: None,
                        severity: None,
                        url: None,
                        labels: Some(labels),
                        help: Some(format!("only numeric types may be used for `{}`", operator)),
                        message: format!("expected numeric types for `{}`", operator),
                    });
                }
            }
            _ => todo!("expect_allowed_binary_operation: {:?}", operator),
        }
    }

    fn expect_match<'a>(&mut self, left: &Type<'a>, right: &Type<'a>) {
        if !left.value.matches(&right.value) {
            let mut labels = vec![];
            labels.extend(left.label(format!("{}", left)));
            labels.extend(right.label(format!("{}", right)));

            self.errors.push(MietteDiagnostic {
                code: None,
                severity: None,
                url: None,
                labels: Some(labels),
                help: Some(format!("{} is not the same type as {}", left, right)),
                message: "type mismatch".to_owned(),
            });
        }
    }
}

struct Environment<'env, 'ast> {
    parent: Option<&'env Environment<'env, 'ast>>,
    bindings: HashMap<Cow<'env, str>, Type<'ast>>,
}

impl<'env, 'ast> Environment<'env, 'ast> {
    fn new(parent: Option<&'env Environment<'env, 'ast>>) -> Self {
        Self {
            parent,
            bindings: HashMap::new(),
        }
    }

    fn set(&mut self, name: Cow<'env, str>, ty: Type<'ast>) {
        self.bindings.insert(name, ty);
    }

    fn get(&self, name: &Cow<'env, str>) -> Option<&Type<'ast>> {
        self.bindings
            .get(name)
            .or_else(|| self.parent.and_then(|p| p.get(name)))
    }
}
