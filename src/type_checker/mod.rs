use crate::{
    expressions::{self, function::FunctionExpression},
    prelude::*,
    statements,
};
pub use environment::Environment;
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

    /// Check a statement with pre-populated declarations from imports
    pub fn check_with_imports(
        &mut self,
        statement: &Statement,
        imported_declarations: &[(String, Type)],
    ) -> Results<TypedStatement> {
        let mut env = Environment::new();

        // Populate environment with imported declarations
        for (name, type_) in imported_declarations {
            let identifier = Identifier::new(name.as_str(), type_.span);
            env.declare(&identifier, type_);
        }

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
            StatementValue::VariableDeclaration(_) => {
                statements::variable_declaration::type_check(self, statement, env)
            }
            StatementValue::ExternDeclaration(_) => {
                statements::extern_declaration::type_check(self, statement, env)
            }
            StatementValue::TypeDeclaration(_) => {
                statements::type_declaration::type_check(self, statement, env)
            }
            StatementValue::Import(_) => statements::import::type_check(self, statement, env),
        }
    }

    pub fn check_expression(
        &mut self,
        expression: &Expression,
        env: &mut Environment,
    ) -> TypedExpression {
        match &expression.value {
            ExpressionValue::Primary(primary) => match primary {
                PrimaryExpression::I32(_) => {
                    expressions::primary::integer::type_check_i32(expression)
                }
                PrimaryExpression::I64(_) => {
                    expressions::primary::integer::type_check_i64(expression)
                }
                PrimaryExpression::Boolean(_) => {
                    expressions::primary::boolean::type_check(expression)
                }
                PrimaryExpression::String(_) => {
                    expressions::primary::string::type_check(self, expression, env)
                }
                PrimaryExpression::Unit => expressions::primary::unit::type_check(expression),
            },
            ExpressionValue::Unary(unary) => match &unary.operator {
                UnaryOperator::Negative => {
                    expressions::unary::negative::type_check(self, expression, env)
                }
                op => todo!("Unary operator {:?} not implemented", op),
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
                BinaryOperator::LessThan => {
                    expressions::binary::less_than::type_check(self, expression, env)
                }
                BinaryOperator::GreaterThan => {
                    expressions::binary::greater_than::type_check(self, expression, env)
                }
                BinaryOperator::GreaterThanOrEqual => {
                    expressions::binary::greater_than_or_equal::type_check(self, expression, env)
                }
                BinaryOperator::Equals => {
                    expressions::binary::equals::type_check(self, expression, env)
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
            ExpressionValue::Conditional(_) => {
                expressions::conditional::type_check(self, expression, env)
            }
            ExpressionValue::StructConstructor(_) => {
                expressions::struct_constructor::type_check(self, expression, env)
            }
            ExpressionValue::FieldAccess(_) => {
                expressions::field_access::type_check(self, expression, env)
            }
            ExpressionValue::Assignment(_) => {
                expressions::assignment::type_check(self, expression, env)
            }
            ExpressionValue::WhileLoop(_) => {
                expressions::while_loop::type_check(self, expression, env)
            }
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

    pub fn expect_type_value(
        &self,
        actual: &Type,
        expected: &TypeValue,
        message: impl Into<String>,
    ) -> TypeValue {
        if &actual.value == expected {
            return actual.value.clone();
        }

        if actual.value == TypeValue::Never || expected == &TypeValue::Never {
            return TypeValue::Never;
        }

        self.errors
            .borrow_mut()
            .push(type_checker_unexpected_type_value(
                expected, actual, message,
            ));

        TypeValue::Never
    }

    pub fn expect_struct_type(&self, actual: &Type, message: impl Into<String>) -> TypeValue {
        if matches!(&actual.value, &TypeValue::Struct(_)) {
            return actual.value.clone();
        }

        if actual.value == TypeValue::Never {
            return TypeValue::Never;
        }

        self.errors
            .borrow_mut()
            .push(type_checker_unexpected_type_value(
                "a struct", actual,
                message, // TODO: the fact we need a separate one for this because the enum has a value is a bit ugly...
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
                .push(declaration_not_found(identifier, message, env));
        }

        type_
    }

    pub fn add_error(&self, error: Error) {
        self.errors.borrow_mut().push(error);
    }

    /// Type-check a function expression with knowledge of its name (for recursion)
    pub fn check_function_with_name(
        &mut self,
        expression: &Expression,
        env: &mut Environment,
        name: &Identifier,
    ) -> TypedExpression {
        let func_expr = match &expression.value {
            ExpressionValue::Function(f) => f,
            _ => unreachable!("check_function_with_name called on non-function"),
        };

        // Infer the function type from its signature
        if let Some(explicit_return_type) = &func_expr.explicit_return_type {
            let function_type = TypeValue::Function(FunctionType {
                parameters: func_expr.parameters.clone(),
                return_type: Box::new(explicit_return_type.clone()),
                span: func_expr.span,
            })
            .with_span(func_expr.span);

            // Create function environment with the function's own name declared
            let mut function_env = env.function();

            // Add the function itself to the environment for recursion
            function_env.declare(name, &function_type);

            // Add parameters
            for parameter in &func_expr.parameters {
                function_env.declare(&parameter.identifier, &parameter.type_);
            }

            // Type-check the body
            let body = self.check_expression(&func_expr.body, &mut function_env);

            // Create the final function type
            let final_type = TypeValue::Function(FunctionType {
                parameters: func_expr.parameters.clone(),
                return_type: Box::new(body.type_.clone()),
                span: func_expr.span,
            });

            // Check that body matches declared return type
            self.expect_same_type(
                vec![&body.type_, explicit_return_type],
                "the function's body should match its explicit return type",
            );

            let value = TypedExpressionValue::Function(FunctionExpression {
                parameters: func_expr.parameters.clone(),
                body: Box::new(body),
                explicit_return_type: func_expr.explicit_return_type.clone(),
                span: func_expr.span,
            });

            expression.with_value_type(value, final_type.with_span(expression.span))
        } else {
            // Fall back to regular function type checking if no explicit return type
            self.check_expression(expression, env)
        }
    }
}
