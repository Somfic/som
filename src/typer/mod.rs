use crate::ast::{
    BinaryOperator, Expression, ExpressionValue, Module, Primitive, Statement, StatementValue,
    TypedExpression, TypedFunctionDeclaration, TypedModule, TypedStatement, Typing, TypingValue,
};
use crate::prelude::*;
use environment::Environment;
use error::mismatched_arguments;
use miette::MietteDiagnostic;

mod environment;
mod error;

pub struct Typer {}

impl Typer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn type_check<'ast>(
        &mut self,
        modules: Vec<Module<'ast>>,
    ) -> ParserResult<Vec<TypedModule<'ast>>> {
        let mut errors = Diagnostics::new();
        let mut environment = Environment::new();
        let mut typed_modules: Vec<TypedModule<'ast>> = Vec::new();

        for module in &modules {
            let module = self.type_check_module(module, &mut environment, &mut errors)?;
            typed_modules.push(module);
        }

        if errors.is_empty() {
            Ok(typed_modules)
        } else {
            Err(errors)
        }
    }

    fn type_check_module<'ast>(
        &mut self,
        module: &Module<'ast>,
        environment: &mut Environment<'_, 'ast>,
        mut errors: &mut Diagnostics,
    ) -> ParserResult<TypedModule<'ast>> {
        let mut typed_functions = vec![];

        for function in &module.functions {
            let dummy = TypedExpression {
                value: ExpressionValue::Primitive(Primitive::Unit), // or another dummy value
                ty: function
                    .return_type
                    .clone()
                    .unwrap_or(Typing::unknown(&function.span)),
                span: function.span,
            };
            let placeholder = TypedFunctionDeclaration {
                name: function.name.clone(),
                span: function.span,
                parameters: function.parameters.clone(),
                body: dummy,
                return_type: function.return_type.clone(),
            };
            environment.declare_function(function.name.clone(), placeholder.clone())?;
        }

        for function in &module.functions {
            // declare the function parameters in the environment
            for (name, ty) in &function.parameters {
                environment.declare_variable(name.clone(), ty.clone());
            }

            let return_value =
                self.type_check_expression(&function.body, &mut environment.clone(), &mut errors)?;

            if let Some(return_type) = &function.return_type {
                if return_value.ty != *return_type {
                    error::new_mismatched_types(
                        "expected the return type to match",
                        &return_value.ty,
                        return_type,
                        format!("{} and {} do not match", return_value.ty, return_type),
                    )
                    .map(|e| errors.add(e));
                }
            }

            // TODO: Add a warning that when using recursive functions, the return type must be explicitly set

            let declaration: crate::ast::GenericFunctionDeclaration<'_, TypedExpression<'_>> =
                TypedFunctionDeclaration {
                    name: function.name.clone(),
                    span: function.span,
                    parameters: function.parameters.clone(),
                    body: return_value,
                    return_type: function.return_type.clone(),
                };

            environment.update_function(function.name.clone(), declaration.clone())?;

            typed_functions.push(declaration);
        }

        Ok(TypedModule {
            functions: typed_functions,
        })
    }

    fn type_check_expression<'ast>(
        &mut self,
        expression: &Expression<'ast>,
        environment: &mut Environment<'_, 'ast>,
        errors: &mut Diagnostics,
    ) -> ParserResult<TypedExpression<'ast>> {
        match &expression.value {
            ExpressionValue::Primitive(primitive) => match primitive {
                Primitive::Integer(_) => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Typing::integer(&expression.span),
                    span: expression.span,
                }),
                Primitive::Decimal(_) => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Typing::decimal(&expression.span),
                    span: expression.span,
                }),
                Primitive::String(_) => todo!("string types"),
                Primitive::Character(_) => todo!("character types"),
                Primitive::Boolean(_) => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Typing::boolean(&expression.span),
                    span: expression.span,
                }),
                Primitive::Unit => Ok(TypedExpression {
                    value: ExpressionValue::Primitive(primitive.clone()),
                    ty: Typing::unit(&expression.span),
                    span: expression.span,
                }),
                Primitive::Identifier(value) => match environment.lookup_variable(value) {
                    Some(ty) => Ok(TypedExpression {
                        value: ExpressionValue::Primitive(primitive.clone()),
                        ty: ty.clone().span(expression.span),
                        span: expression.span,
                    }),
                    None => {
                        errors.add(error::undefined_variable(
                            format!("the identifier {value} is not defined"),
                            value,
                            expression.span,
                        ));
                        Ok(TypedExpression {
                            value: ExpressionValue::Primitive(primitive.clone()),
                            ty: Typing::unknown(&expression.span),
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
                let left = self.type_check_expression(left, environment, errors)?;
                let right = self.type_check_expression(right, environment, errors)?;
                let left_ty = left.ty.clone();

                if left_ty != right.ty {
                    error::new_mismatched_types(
                        format!("expected the types between {operator} to match"),
                        &left_ty,
                        &right.ty,
                        format!("{left_ty} and {} do not match", right.ty),
                    )
                    .map(|e| errors.add(e));
                }

                let ty = if operator.is_logical() {
                    Typing::boolean(&expression.span)
                } else {
                    left_ty
                };

                Ok(TypedExpression {
                    value: ExpressionValue::Binary {
                        operator: operator.clone(),
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    ty,
                    span: expression.span,
                })
            }
            ExpressionValue::Group(expression) => {
                self.type_check_expression(expression, environment, errors)
            }
            ExpressionValue::Unary { operator, operand } => match operator {
                crate::ast::UnaryOperator::Negate => todo!(),
                crate::ast::UnaryOperator::Negative => Ok(TypedExpression {
                    value: ExpressionValue::Unary {
                        operator: operator.clone(),
                        operand: Box::new(self.type_check_expression(
                            operand,
                            environment,
                            errors,
                        )?),
                    },
                    ty: Typing::integer(&expression.span),
                    span: expression.span,
                }),
            },
            ExpressionValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let condition = self.type_check_expression(condition, environment, errors)?;
                let truthy = self.type_check_expression(truthy, environment, errors)?;
                let falsy = self.type_check_expression(falsy, environment, errors)?;
                let truthy_ty = truthy.ty.clone();

                if condition.ty.value != TypingValue::Boolean {
                    error::new_mismatched_types(
                        "expected the condition to be a boolean",
                        &condition.ty,
                        &Typing::boolean(&condition.span),
                        format!("{} is not a boolean", condition.ty),
                    )
                    .map(|e| errors.add(e));
                }

                if truthy_ty != falsy.ty {
                    error::new_mismatched_types(
                        "expected the types of the truthy and falsy branches to match",
                        &truthy.ty,
                        &falsy.ty,
                        format!("{} and {} do not match", truthy.ty, falsy.ty),
                    )
                    .map(|e| errors.add(e));
                }

                Ok(TypedExpression {
                    ty: truthy_ty.span(truthy.span),
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
                    .map(|statement| self.type_check_statement(statement, environment, errors))
                    .collect::<ParserResult<Vec<_>>>()?;

                let result = self.type_check_expression(result, environment, errors)?;
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
            ExpressionValue::FunctionCall {
                function_name,
                arguments,
            } => {
                let function = environment
                    .lookup_function(function_name)
                    .ok_or_else(|| {
                        errors.add(error::undefined_function(
                            format!("the function {function_name} is not defined"),
                            function_name,
                            expression.span,
                        ))
                    })
                    .unwrap();

                if function.parameters.len() != arguments.len() {
                    errors.add(mismatched_arguments(
                        format!(
                            "expected {} arguments, but got {}",
                            function.parameters.len(),
                            arguments.len()
                        ),
                        arguments.clone(),
                        function.parameters.values().cloned().collect::<Vec<_>>(),
                        format!(
                            "expected {} but got {}",
                            function.parameters.len(),
                            arguments.len()
                        ),
                    ));
                }

                let mut typed_arguments = Vec::new();
                let expected_types: Vec<_> = function.parameters.values().cloned().collect();
                for (i, argument) in arguments.iter().enumerate() {
                    let argument =
                        self.type_check_expression(argument, &mut environment.clone(), errors)?;
                    let expected_ty = &expected_types
                        .get(i)
                        .cloned()
                        .unwrap_or(Typing::unknown(&argument.span));

                    if argument.ty != *expected_ty {
                        error::new_mismatched_types(
                            format!("expected the type of argument {i} to be {expected_ty}"),
                            &argument.ty,
                            expected_ty,
                            format!("{} and {} do not match", argument.ty, expected_ty),
                        )
                        .map(|e| errors.add(e));
                    }

                    typed_arguments.push(argument);
                }

                Ok(TypedExpression {
                    value: ExpressionValue::FunctionCall {
                        function_name: function_name.clone(),
                        arguments: typed_arguments,
                    },
                    ty: function.body.ty.clone(),
                    span: expression.span,
                })
            }
            ExpressionValue::Assignment { name, value } => {
                let value = self.type_check_expression(value, environment, errors)?;
                environment.assign_variable(name.clone(), value.ty.clone());

                Ok(TypedExpression {
                    value: ExpressionValue::Assignment {
                        name: name.clone(),
                        value: Box::new(value),
                    },
                    ty: Typing::unknown(&expression.span),
                    span: expression.span,
                })
            }
        }
    }

    fn type_check_statement<'ast>(
        &mut self,
        statement: &Statement<'ast>,
        environment: &mut Environment<'_, 'ast>,
        errors: &mut Diagnostics,
    ) -> ParserResult<TypedStatement<'ast>> {
        match &statement.value {
            StatementValue::Block(statements) => {
                let environment = &mut environment.block();

                let statements = statements
                    .iter()
                    .map(|statement| self.type_check_statement(statement, environment, errors))
                    .collect::<ParserResult<Vec<_>>>()?;

                Ok(TypedStatement {
                    value: StatementValue::Block(statements),
                    span: statement.span,
                })
            }
            StatementValue::Expression(expression) => {
                let expression = self.type_check_expression(expression, environment, errors)?;
                Ok(TypedStatement {
                    value: StatementValue::Expression(expression),
                    span: statement.span,
                })
            }
            StatementValue::Declaration(name, expression) => {
                let expression = self.type_check_expression(expression, environment, errors)?;

                environment.declare_variable(name.clone(), expression.ty.clone());

                Ok(TypedStatement {
                    value: StatementValue::Declaration(name.clone(), expression),
                    span: statement.span,
                })
            }
            StatementValue::Condition(condition, statement) => {
                let condition = self.type_check_expression(condition, environment, errors)?;
                let statement = self.type_check_statement(statement, environment, errors)?;

                if condition.ty.value != TypingValue::Boolean {
                    error::new_mismatched_types(
                        "expected the condition to be a boolean",
                        &condition.ty,
                        &Typing::boolean(&condition.span),
                        format!("{} is not a boolean", condition.ty),
                    )
                    .map(|e| errors.add(e));
                }

                Ok(TypedStatement {
                    span: condition.span,
                    value: StatementValue::Condition(condition, Box::new(statement)),
                })
            }
        }
    }
}
