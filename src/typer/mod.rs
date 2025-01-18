use crate::parser::ast::{
    BinaryOperator, CombineSpan, Expression, ExpressionValue, Primitive, Statement, StatementValue,
    Type, TypeValue,
};
use environment::Environment;
use miette::{MietteDiagnostic, Result, SourceSpan};

pub mod environment;
#[cfg(test)]
mod tests;
pub struct TypeChecker<'ast> {
    statements: &'ast Vec<Statement<'ast>>,
    errors: Vec<MietteDiagnostic>,
}

impl<'ast> TypeChecker<'ast> {
    pub fn new(statements: &'ast Vec<Statement<'ast>>) -> Self {
        Self {
            statements,
            errors: vec![],
        }
    }

    pub fn check(mut self) -> Vec<MietteDiagnostic> {
        // Initially, there is no parent environment
        let mut environment = Environment::new(None);

        for statement in self.statements {
            self.check_statement(&statement, &mut environment);
        }

        self.errors
    }

    fn check_statement<'env>(
        &mut self,
        statement: &Statement<'ast>,
        environment: &mut Environment<'env, 'ast>,
    ) {
        match &statement.value {
            StatementValue::Expression(expr) => {
                self.check_expression(expr, environment);
            }
            StatementValue::Block(statements) => {
                // Create a new child environment
                let mut environment = Environment::new(Some(environment));

                for stmt in statements {
                    self.check_statement(stmt, &mut environment);
                }
            }
            StatementValue::Function { header, body } => {
                environment.set(
                    header.name.clone(),
                    Type::function(
                        header.span,
                        header
                            .parameters
                            .iter()
                            .map(|p| p.explicit_type.clone())
                            .collect(),
                        header
                            .explicit_return_type
                            .clone()
                            .unwrap_or(Type::unit(header.span)),
                    ),
                );

                header.parameters.iter().for_each(|p| {
                    environment.set(p.name.clone(), p.explicit_type.clone());
                });

                let implicit_return_type = self
                    .check_expression(body, environment)
                    .unwrap_or(Type::unit(body.span));

                self.expect_match(
                    &header
                        .explicit_return_type
                        .clone()
                        .unwrap_or(Type::unit(header.span)),
                    &implicit_return_type,
                    "explicit and implicit return types must match".into(),
                );
            }
            StatementValue::Return(expr) => {
                self.check_expression(expr, environment);
            }
            StatementValue::Enum {
                name: _,
                variants: _,
            } => {
                // Not implemented yet
            }
            StatementValue::Struct { name: _, fields: _ } => {
                // Not implemented yet
            }
            StatementValue::Assignment { name, value } => {
                if let Some(expression_type) = self.check_expression(value, environment) {
                    environment.set(name.clone(), expression_type);
                }
            }
            StatementValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let condition = self
                    .check_expression(condition, environment)
                    .unwrap_or(Type::unit(condition.span));

                self.expect_type(
                    &condition,
                    TypeValue::Boolean,
                    "the condition must be boolean".into(),
                );

                self.check_statement(truthy, environment);

                if let Some(falsy) = falsy {
                    self.check_statement(falsy, environment);
                }
            }
            StatementValue::Trait {
                name: _,
                functions: _,
            } => todo!(),
            StatementValue::TypeAlias {
                name,
                explicit_type,
            } => {
                environment.set(
                    name.clone(),
                    Type::alias(explicit_type.span, name.clone(), explicit_type.clone()),
                );
            }
        };
    }

    fn check_expression<'env>(
        &mut self,
        expression: &Expression<'ast>,
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
        expression: &Expression<'ast>,
        environment: &Environment<'env, 'ast>,
    ) -> Result<Type<'ast>, Vec<MietteDiagnostic>> {
        match &expression.value {
            ExpressionValue::Primitive(primitive) => match primitive {
                Primitive::Integer(_) => Ok(Type::integer(expression.span)),
                Primitive::Decimal(_) => Ok(Type::decimal(expression.span)),
                Primitive::Boolean(_) => Ok(Type::boolean(expression.span)),
                Primitive::String(_) => Ok(Type::string(expression.span)),
                Primitive::Identifier(name) => environment
                    .get(name)
                    .cloned()
                    .map(|ty| ty.span(expression.span))
                    .ok_or_else(|| {
                        vec![MietteDiagnostic {
                            code: None,
                            severity: None,
                            url: None,
                            labels: Some(vec![expression.label("undeclared variable")]),
                            help: Some(format!("{} is not declared", name)),
                            message: "undeclared variable".to_owned(),
                        }]
                    }),
                Primitive::Character(_) => Ok(Type::character(expression.span)),
                Primitive::Unit => Ok(Type::unit(expression.span)),
            },
            ExpressionValue::Group(expr) => self.type_of(expr, environment),
            ExpressionValue::Block {
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
            ExpressionValue::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.type_of(left, environment)?;
                let right = self.type_of(right, environment)?;
                self.expect_match(
                    &left,
                    &right,
                    "left and right must be of the same type".into(),
                );
                self.expect_allowed_binary_operation(&left, &right, operator);

                if operator.is_comparison() {
                    Ok(Type::boolean(expression.span))
                } else {
                    Ok(left
                        .clone()
                        .span(SourceSpan::combine(vec![left.span, right.span])))
                }
            }
            ExpressionValue::Unary {
                operator: _,
                operand,
            } => self.type_of(operand, environment),
            ExpressionValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let condition = self.type_of(condition, environment)?;
                let truthy = self.type_of(truthy, environment)?;
                let falsy = self.type_of(falsy, environment)?;

                self.expect_type(
                    &condition,
                    TypeValue::Boolean,
                    "the condition must be boolean".into(),
                );
                self.expect_match(
                    &truthy,
                    &falsy,
                    "truthy and falsy branches must be of the same type".into(),
                );

                Ok(truthy)
            }
            ExpressionValue::Call { callee, arguments } => {
                let callee = self.type_of(callee, environment)?;

                match callee.clone().value {
                    TypeValue::Function {
                        parameters,
                        return_type,
                    } => {
                        if parameters.len() != arguments.len() {
                            return Err(vec![MietteDiagnostic {
                                code: None,
                                severity: None,
                                url: None,
                                labels: Some(callee.label("function call")),
                                help: Some(format!(
                                    "expected {} arguments, but found {}",
                                    parameters.len(),
                                    arguments.len()
                                )),
                                message: "incorrect number of arguments".to_owned(),
                            }]);
                        }

                        for (parameter, argument) in parameters.iter().zip(arguments) {
                            let argument = self.type_of(argument, environment)?;

                            self.expect_match(
                                parameter,
                                &argument,
                                "argument and parameter must match".into(),
                            );
                        }

                        Ok((return_type.span(callee.span)).clone())
                    }
                    _ => Err(vec![MietteDiagnostic {
                        code: None,
                        severity: None,
                        url: None,
                        labels: Some(callee.label("function call")),
                        help: Some("only functions may be called".into()),
                        message: "not a function".to_owned(),
                    }]),
                }
            }
            ExpressionValue::Lambda(lambda) => {
                let mut environment = Environment::new(Some(environment));

                for parameter in &lambda.parameters {
                    environment.set(parameter.name.clone(), parameter.explicit_type.clone());
                }

                let body = self.type_of(&lambda.body, &environment)?;

                Ok(Type::function(
                    expression.span,
                    lambda
                        .parameters
                        .iter()
                        .map(|p| p.explicit_type.clone())
                        .collect(),
                    body,
                ))
            }
        }
    }

    fn expect_allowed_binary_operation(
        &mut self,
        left: &Type<'ast>,
        right: &Type<'ast>,
        operator: &BinaryOperator,
    ) {
        match operator {
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Modulo
            | BinaryOperator::LessThan
            | BinaryOperator::LessThanOrEqual
            | BinaryOperator::GreaterThan
            | BinaryOperator::GreaterThanOrEqual => {
                // if !left.value.is_numeric() || !right.value.is_numeric() {
                //     let mut labels = vec![];
                //     if !left.value.is_numeric() {
                //         labels.extend(
                //             left.label(format!("{}", left)),
                //         );
                //     }
                //     if !right.value.is_numeric() {
                //         labels.extend(
                //             right.label(format!("{}", right)),
                //         );
                //     }

                //     self.errors.push(MietteDiagnostic {
                //         code: None,
                //         severity: None,
                //         url: None,
                //         labels: Some(labels),
                //         help: Some(format!("only numeric types may be used for {}", operator)),
                //         message: format!("type mismatch, expected numeric types for {}, but found ", operator),
                //     });
                // }
                self.expect_types(
                    left,
                    &[TypeValue::Integer, TypeValue::Decimal],
                    "left side must be a numeric type".into(),
                );

                self.expect_types(
                    right,
                    &[TypeValue::Integer, TypeValue::Decimal],
                    "right side must be a numeric type".into(),
                );
            }
            BinaryOperator::Equality | BinaryOperator::Inequality => {
                // TODO: Implement equality and inequality
            }
            _ => todo!("expect_allowed_binary_operation: {:?}", operator),
        }
    }

    fn expect_match(&mut self, left: &Type<'ast>, right: &Type<'ast>, message: String) {
        if left != right {
            let mut labels = vec![];
            labels.extend(left.label(format!("{}", left)));
            labels.extend(right.label(format!("{}", right)));

            self.errors.push(MietteDiagnostic {
                code: None,
                severity: None,
                url: None,
                labels: Some(labels),
                help: Some(format!(
                    "type mismatch, expected types to be equivalent, but found {} and {}",
                    left, right
                )),
                message,
            });
        }
    }

    fn expect_type(&mut self, ty: &Type<'ast>, expected: TypeValue, message: String) {
        self.expect_types(ty, &[expected], message);
    }

    fn expect_types(&mut self, ty: &Type<'ast>, expected: &[TypeValue], message: String) {
        if !expected.iter().any(|ex| ty.value == *ex) {
            let mut labels = vec![];
            labels.extend(ty.label(format!("{}", ty)));

            self.errors.push(MietteDiagnostic {
                code: None,
                severity: None,
                url: None,
                labels: Some(labels),
                help: Some(format!(
                    "unexpected type, expected {} but found {}",
                    expected
                        .iter()
                        .map(|ex| ex.to_string())
                        .collect::<Vec<String>>()
                        .join(" or "),
                    ty
                )),
                message,
            });
        }
    }
}
