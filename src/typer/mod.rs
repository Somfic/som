use std::borrow::Cow;
use std::cmp;

use crate::ast::{
    CombineSpan, Expression, ExpressionValue, FunctionDeclaration, IntrinsicFunctionDeclaration,
    Module, Primitive, Spannable, Statement, StatementValue, TypedExpression,
    TypedFunctionDeclaration, TypedModule, TypedStatement, Typing, TypingValue,
};
use crate::prelude::*;
use environment::Environment;
use miette::MietteDiagnostic;

pub mod environment;
mod error;

pub struct Typer {
    errors: Vec<MietteDiagnostic>,
}

impl Typer {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn type_check<'ast>(
        &mut self,
        modules: Vec<Module<'ast>>,
    ) -> ParserResult<Vec<TypedModule<'ast>>> {
        let mut environment = Environment::new();

        let mut typed_modules: Vec<TypedModule<'ast>> = Vec::new();

        for module in &modules {
            let module = self.type_check_module(module, &mut environment)?;
            typed_modules.push(module);
        }

        if self.errors.is_empty() {
            Ok(typed_modules)
        } else {
            Err(self.errors.clone())
        }
    }

    fn report_errors(&mut self, errors: Vec<MietteDiagnostic>) {
        self.errors.extend(errors);
    }

    fn report_error(&mut self, error: Option<MietteDiagnostic>) {
        if let Some(error) = error {
            self.errors.push(error);
        }
    }

    fn type_check_module<'ast>(
        &mut self,
        module: &Module<'ast>,
        environment: &mut Environment<'_, 'ast>,
    ) -> ParserResult<TypedModule<'ast>> {
        let mut typed_functions = vec![];

        for function in &module.intrinsic_functions {
            self.declare_intrinsic_function(function, environment)?;
        }

        for intrinsic_function in &module.functions {
            self.declare_function(intrinsic_function, environment)?;
        }

        for function in &module.functions {
            typed_functions.push(self.type_check_function(function, environment)?);
        }

        Ok(TypedModule {
            functions: typed_functions,
            intrinsic_functions: module.intrinsic_functions.clone(),
        })
    }

    fn type_check_expression<'ast>(
        &mut self,
        expression: &Expression<'ast>,
        environment: &mut Environment<'_, 'ast>,
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
                        ty: ty.clone().with_span(expression.span),
                        span: expression.span,
                    }),
                    None => {
                        self.report_error(error::undefined_variable(
                            format!("variable `{value}` is not defined"),
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
                let left = self.type_check_expression(left, environment)?;
                let right = self.type_check_expression(right, environment)?;

                if !types_match(&left.ty, &right.ty, environment)? {
                    self.report_error(error::new_mismatched_types(
                        format!("expected the types between {operator} to match"),
                        &left.ty,
                        &right.ty,
                        format!("{} and {} do not match", left.ty, right.ty),
                    ));
                }

                let ty = if operator.is_logical() {
                    Typing::boolean(&expression.span)
                } else {
                    left.ty.clone()
                };

                let span = left.span.combine(right.span);

                Ok(TypedExpression {
                    value: ExpressionValue::Binary {
                        operator: operator.clone(),
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    ty,
                    span,
                })
            }
            ExpressionValue::Group(expression) => {
                self.type_check_expression(expression, environment)
            }
            ExpressionValue::Unary { operator, operand } => match operator {
                crate::ast::UnaryOperator::Negate => Ok(TypedExpression {
                    // TODO: check if the operand is a boolean
                    value: ExpressionValue::Unary {
                        operator: operator.clone(),
                        operand: Box::new(self.type_check_expression(operand, environment)?),
                    },
                    ty: Typing::boolean(&expression.span),
                    span: expression.span,
                }),
                crate::ast::UnaryOperator::Negative => Ok(TypedExpression {
                    // TODO: check if its a number
                    value: ExpressionValue::Unary {
                        operator: operator.clone(),
                        operand: Box::new(self.type_check_expression(operand, environment)?),
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
                let condition = self.type_check_expression(condition, environment)?;
                let truthy = self.type_check_expression(truthy, environment)?;
                let falsy = self.type_check_expression(falsy, environment)?;
                let truthy_ty = truthy.ty.clone();

                if !type_matches(&condition.ty, TypingValue::Boolean, environment)? {
                    self.report_error(error::new_mismatched_types(
                        "expected the condition to be a boolean",
                        &condition.ty,
                        &Typing::boolean(&condition.span),
                        format!("{} is not a boolean", condition.ty),
                    ));
                }

                if !types_match(&truthy.ty, &falsy.ty, environment)? {
                    self.report_error(error::new_mismatched_types(
                        "expected the types of the truthy and falsy branches to match",
                        &truthy.ty,
                        &falsy.ty,
                        format!("{} and {} do not match", truthy.ty, falsy.ty),
                    ));
                }

                Ok(TypedExpression {
                    ty: truthy_ty.with_span(truthy.span),
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
                    .map(|statement| self.type_check_statement(statement, environment))
                    .collect::<ParserResult<Vec<_>>>()?;

                let result = self.type_check_expression(result, environment)?;
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
                let function = environment.lookup_function(function_name).ok_or_else(|| {
                    vec![error::undefined_function(
                        format!("function `{function_name}` is not defined"),
                        function_name,
                        expression.span,
                    )
                    .unwrap()]
                })?;

                let mut environment = environment.clone();
                let results = arguments
                    .iter()
                    .map(|a| self.type_check_expression(a, &mut environment))
                    .collect::<Vec<_>>();

                let typed_arguments = results
                    .iter()
                    .filter_map(|r| {
                        if let Err(e) = r {
                            self.report_errors(e.clone());
                            None
                        } else {
                            r.as_ref().ok()
                        }
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                for i in 0..cmp::max(arguments.len(), function.parameters.len()) {
                    let argument = typed_arguments.get(i);
                    let parameter = function.parameters.get(i);

                    if argument.is_some() && parameter.is_none() {
                        let argument = argument.unwrap();

                        self.report_error(error::unexpected_argument(
                            "unexpected argument",
                            function,
                            argument,
                            format!(
                                "the function `{}` requires {} arguments but {} were given",
                                function_name,
                                function.parameters.len(),
                                arguments.len()
                            ),
                        ));
                    }

                    if argument.is_none() && parameter.is_some() {
                        let parameter = parameter.unwrap();

                        self.report_error(error::missing_argument(
                            format!("missing argument for `{}`", parameter.name),
                            expression,
                            parameter,
                            format!(
                                "the function `{}` requires the `{}` parameter but it was not given",
                                function_name,
                                parameter.name
                            ),
                        ));
                    };

                    if argument.is_some() && parameter.is_some() {
                        let argument = argument.unwrap();
                        let parameter = parameter.unwrap();

                        if !types_match(&argument.ty, &parameter.ty, &environment)? {
                            self.report_error(error::mismatched_argument(
                                format!("mismatching argument type for `{}`", parameter.name),
                                argument,
                                parameter,
                                format!(
                                    "the function `{}` requires the `{}` parameter to be {} but it was {}",
                                    function_name, parameter.name, parameter.ty, argument.ty
                                ),
                            ));
                        }
                    }
                }

                Ok(TypedExpression {
                    value: ExpressionValue::FunctionCall {
                        function_name: function_name.clone(),
                        arguments: typed_arguments,
                    },
                    ty: function.body.ty.clone().with_span(expression.span),
                    span: expression.span,
                })
            }
            ExpressionValue::Assignment { name, value } => {
                let value = self.type_check_expression(value, environment)?;
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
    ) -> ParserResult<TypedStatement<'ast>> {
        match &statement.value {
            StatementValue::Block(statements) => {
                let environment = &mut environment.block();

                let statements = statements
                    .iter()
                    .map(|statement| self.type_check_statement(statement, environment))
                    .collect::<ParserResult<Vec<_>>>()?;

                Ok(TypedStatement {
                    value: StatementValue::Block(statements),
                    span: statement.span,
                })
            }
            StatementValue::Expression(expression) => {
                let expression = self.type_check_expression(expression, environment)?;
                Ok(TypedStatement {
                    value: StatementValue::Expression(expression),
                    span: statement.span,
                })
            }
            StatementValue::VariableDeclaration(name, explicit_type, expression) => {
                let expression = self.type_check_expression(expression, environment)?;

                if let Some(explicit_type) = explicit_type {
                    if !types_match(&expression.ty, explicit_type, environment)? {
                        self.report_error(error::new_mismatched_types(
                            "expected the types to match",
                            &expression.ty,
                            explicit_type,
                            format!("{} and {} do not match", expression.ty, explicit_type),
                        ));
                    }
                }

                environment.declare_variable(name.clone(), expression.ty.clone());

                Ok(TypedStatement {
                    value: StatementValue::VariableDeclaration(
                        name.clone(),
                        explicit_type.clone(),
                        expression,
                    ),
                    span: statement.span,
                })
            }
            StatementValue::Condition(condition, statement) => {
                let condition = self.type_check_expression(condition, environment)?;
                let statement = self.type_check_statement(statement, environment)?;

                if !type_matches(&condition.ty, TypingValue::Boolean, environment)? {
                    self.report_error(error::new_mismatched_types(
                        "expected the condition to be a boolean",
                        &condition.ty,
                        &Typing::boolean(&condition.span),
                        format!("{} is not a boolean", condition.ty),
                    ));
                }

                Ok(TypedStatement {
                    span: condition.span,
                    value: StatementValue::Condition(condition, Box::new(statement)),
                })
            }
            StatementValue::WhileLoop(condition, statement) => {
                let condition = self.type_check_expression(condition, environment)?;
                let statement = self.type_check_statement(statement, environment)?;

                if !type_matches(&condition.ty, TypingValue::Boolean, environment)? {
                    self.report_error(error::new_mismatched_types(
                        "expected the condition of loop to be a boolean",
                        &condition.ty,
                        &Typing::boolean(&condition.span),
                        format!("{} is not a boolean", condition.ty),
                    ));
                }

                Ok(TypedStatement {
                    span: condition.span,
                    value: StatementValue::WhileLoop(condition, Box::new(statement)),
                })
            }
            StatementValue::FunctionDeclaration(function) => {
                self.declare_function(function, environment)?;
                let function = self.type_check_function(function, environment)?;

                Ok(TypedStatement {
                    span: function.span,
                    value: StatementValue::FunctionDeclaration(function),
                })
            }
            StatementValue::IntrinsicDeclaration(intrinsic) => {
                self.declare_intrinsic_function(intrinsic, environment)?;

                Ok(TypedStatement {
                    span: intrinsic.span,
                    value: StatementValue::IntrinsicDeclaration(intrinsic.clone()),
                })
            }
            StatementValue::TypeDeclaration(identifier, ty) => {
                let ty = ty.clone().with_span(statement.span);
                environment.declare_type(identifier.clone(), ty.clone())?;

                Ok(TypedStatement {
                    span: statement.span,
                    value: StatementValue::TypeDeclaration(identifier.clone(), ty),
                })
            }
        }
    }

    fn declare_function<'ast>(
        &mut self,
        function: &FunctionDeclaration<'ast>,
        environment: &mut Environment<'_, 'ast>,
    ) -> ParserResult<()> {
        let dummy = TypedExpression {
            value: ExpressionValue::Primitive(Primitive::Unit),
            ty: function
                .explicit_return_type
                .clone()
                .unwrap_or(Typing::unknown(&function.span)),
            span: function.span,
        };
        let placeholder = TypedFunctionDeclaration {
            name: function.name.clone(),
            span: function.span,
            parameters: function.parameters.clone(),
            body: dummy,
            explicit_return_type: function.explicit_return_type.clone(),
        };
        environment.declare_function(function.name.clone(), placeholder.clone())?;

        Ok(())
    }

    fn declare_intrinsic_function<'ast>(
        &mut self,
        function: &IntrinsicFunctionDeclaration<'ast>,
        environment: &mut Environment<'_, 'ast>,
    ) -> ParserResult<()> {
        let dummy = TypedExpression {
            value: ExpressionValue::Primitive(Primitive::Unit),
            ty: function.return_type.clone(),
            span: function.span,
        };
        let placeholder = TypedFunctionDeclaration {
            name: function.name.clone(),
            span: function.span,
            parameters: function.parameters.clone(),
            body: dummy,
            explicit_return_type: Some(function.return_type.clone()),
        };
        environment.declare_function(function.name.clone(), placeholder.clone())?;

        Ok(())
    }

    fn type_check_function<'ast>(
        &mut self,
        function: &FunctionDeclaration<'ast>,
        environment: &mut Environment<'_, 'ast>,
    ) -> ParserResult<TypedFunctionDeclaration<'ast>> {
        for parameter in &function.parameters {
            environment.declare_variable(parameter.name.clone(), parameter.ty.clone());
        }

        let return_value = self.type_check_expression(&function.body, &mut environment.clone())?;

        if let Some(return_type) = &function.explicit_return_type {
            if return_value.ty != *return_type {
                self.report_error(error::new_mismatched_types(
                    "expected the return type to match",
                    &return_value.ty,
                    return_type,
                    format!(
                        "the function `{}` requires the return type to be {}, but {} was returned",
                        function.name, return_type, return_value.ty,
                    ),
                ));
            }
        }

        // TODO: Add a warning that when using recursive functions, the return type must be explicitly set

        let declaration: crate::ast::GenericFunctionDeclaration<'_, TypedExpression<'_>> =
            TypedFunctionDeclaration {
                name: function.name.clone(),
                span: function.span,
                parameters: function.parameters.clone(),
                body: return_value,
                explicit_return_type: function.explicit_return_type.clone(),
            };

        environment.update_function(function.name.clone(), declaration.clone())?;

        Ok(declaration)
    }
}

fn types_match(a: &Typing, b: &Typing, environment: &Environment) -> ParserResult<bool> {
    let mut errors = vec![];

    let a_matches = match type_matches(a, b.value.clone(), environment) {
        Ok(a) => a,
        Err(e) => {
            errors.extend(e);
            false
        }
    };

    let b_matches = match type_matches(b, a.value.clone(), environment) {
        Ok(b) => b,
        Err(e) => {
            errors.extend(e);
            false
        }
    };

    if errors.is_empty() {
        Ok(a_matches && b_matches)
    } else {
        Err(errors)
    }
}

fn type_matches(ty: &Typing, value: TypingValue, environment: &Environment) -> ParserResult<bool> {
    let (unzipped_ty, identifier_name) = match &ty.value {
        TypingValue::Symbol(identifier) => (environment.lookup_type(identifier), identifier),
        _ => (Some(ty), &Cow::Owned(format!("{ty}"))),
    };

    if unzipped_ty.is_none() {
        return Err(vec![error::undefined_type(
            format!("type `{}` is not defined", ty.value),
            identifier_name,
            ty.span,
        )]);
    }

    let ty = unzipped_ty.unwrap();

    if ty.value == TypingValue::Unknown {
        return Ok(false);
    }

    Ok(ty.value == value)
}
