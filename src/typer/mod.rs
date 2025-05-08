use crate::ast::{
    CombineSpan, Expression, ExpressionValue, Function, Identifier, IntrinsicSignature,
    LambdaSignature, Module, Primitive, Statement, StatementValue, TypedExpression, TypedFunction,
    TypedModule, TypedStatement, Typing, TypingValue,
};
use crate::parser::ParserResult;
use crate::prelude::*;
use environment::Environment;
use miette::MietteDiagnostic;
use std::cell::RefCell;
use std::cmp;
use std::collections::HashMap;

mod environment;
mod error;

pub struct Typer {
    errors: RefCell<Vec<MietteDiagnostic>>,
    parsed: ParserResult,
}

pub struct TyperResult {
    pub modules: Vec<TypedModule>,
}

impl Typer {
    pub fn new(parsed: ParserResult) -> Self {
        Self {
            errors: RefCell::new(Vec::new()),
            parsed,
        }
    }

    pub fn type_check(mut self) -> Result<TyperResult> {
        let mut environment = Environment::new();

        let mut modules: Vec<TypedModule> = Vec::new();

        for module in self.parsed.modules.clone() {
            let type_checked_module = self.type_check_module(&module, &mut environment)?;
            modules.push(type_checked_module);
        }

        let errors = self.errors.borrow();
        if errors.is_empty() {
            Ok(TyperResult { modules })
        } else {
            Err(errors.clone())
        }
    }

    fn report_errors(&self, errors: Vec<MietteDiagnostic>) {
        self.errors.borrow_mut().extend(errors);
    }

    fn report_error(&self, error: Option<MietteDiagnostic>) {
        if let Some(error) = error {
            self.errors.borrow_mut().push(error);
        }
    }

    fn type_check_module(
        &mut self,
        module: &Module,
        environment: &mut Environment,
    ) -> Result<TypedModule> {
        let mut statements = vec![];

        for statement in &module.statements {
            statements.push(self.type_check_statement(&statement, environment)?);
        }

        Ok(TypedModule { statements })
    }

    fn type_check_expression(
        &mut self,
        expression: &Expression,
        environment: &mut Environment,
    ) -> Result<TypedExpression> {
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
                Primitive::Identifier(identifier) => {
                    match environment.lookup_variable(identifier) {
                        Some(ty) => Ok(TypedExpression {
                            value: ExpressionValue::Primitive(primitive.clone()),
                            ty: ty.clone().with_span(expression.span),
                            span: expression.span,
                        }),
                        None => {
                            self.report_error(error::undefined_variable(
                                format!("variable `{identifier}` is not defined"),
                                identifier,
                                expression.span,
                            ));
                            Ok(TypedExpression {
                                value: ExpressionValue::Primitive(primitive.clone()),
                                ty: Typing::unknown(&expression.span),
                                span: expression.span,
                            })
                        }
                    }
                }
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
                let mut typed_statements = Vec::new();
                for statement in statements.iter() {
                    typed_statements.push(self.type_check_statement(statement, environment)?);
                }
                let statements = typed_statements;

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
                identifier,
                arguments,
            } => {
                let function = environment.lookup_function(identifier).ok_or_else(|| {
                    vec![error::undefined_function(
                        format!("function `{identifier}` is not defined"),
                        identifier,
                        expression.span,
                    )
                    .unwrap()]
                })?;

                let mut environment = environment.block();
                let mut results = Vec::new();
                for arg in arguments.iter() {
                    results.push(self.type_check_expression(arg, &mut environment));
                }

                let mut typed_arguments = Vec::new();
                for r in results {
                    match r {
                        Ok(expr) => typed_arguments.push(expr),
                        Err(errors) => self.report_errors(errors),
                    }
                }

                for i in 0..cmp::max(arguments.len(), function.signature.parameters.len()) {
                    let argument = typed_arguments.get(i);
                    let parameter = function.signature.parameters.get(i);

                    if argument.is_some() && parameter.is_none() {
                        let argument = argument.unwrap();

                        self.report_error(error::unexpected_argument(
                            "unexpected argument",
                            &function.signature,
                            argument,
                            format!(
                                "the function `{}` requires {} arguments but {} were given",
                                identifier,
                                function.signature.parameters.len(),
                                arguments.len()
                            ),
                        ));
                    }

                    if argument.is_none() && parameter.is_some() {
                        let parameter = parameter.unwrap();

                        self.report_error(error::missing_argument(
                                                            format!("missing argument for `{}`", parameter.identifier),
                                                            expression,
                                                            parameter,
                                                            format!(
                                                                "the function `{}` requires the `{}` parameter but it was not given",
                                                                identifier,
                                                                parameter.identifier
                                                            ),
                                                        ));
                    };

                    if argument.is_some() && parameter.is_some() {
                        let argument = argument.unwrap();
                        let parameter = parameter.unwrap();

                        if !types_match(&argument.ty, &parameter.ty, &environment)? {
                            self.report_error(error::mismatched_argument(
                                                                format!("mismatching argument type for `{}`", parameter.identifier),
                                                                argument,
                                                                parameter,
                                                                format!(
                                                                    "the function `{}` requires the `{}` parameter to be {} but it was {}",
                                                                    identifier, parameter.identifier, parameter.ty, argument.ty
                                                                ),
                                                            ));
                        }
                    }
                }

                Ok(TypedExpression {
                    value: ExpressionValue::FunctionCall {
                        identifier: identifier.clone(),
                        arguments: typed_arguments,
                    },
                    ty: function.body.ty.clone().with_span(expression.span),
                    span: expression.span,
                })
            }
            ExpressionValue::VariableAssignment {
                identifier: name,
                argument: value,
            } => {
                let value = self.type_check_expression(value, environment)?;
                environment.assign_variable(name, &value.ty);

                Ok(TypedExpression {
                    value: ExpressionValue::VariableAssignment {
                        identifier: name.clone(),
                        argument: Box::new(value),
                    },
                    ty: Typing::unknown(&expression.span),
                    span: expression.span,
                })
            }
            ExpressionValue::StructConstructor {
                identifier,
                arguments: _arguments,
            } => {
                let _ty = environment.lookup_type(identifier).ok_or_else(|| {
                    vec![error::undefined_type(
                        format!("type `{identifier}` is not defined"),
                        identifier,
                        expression.span,
                    )]
                })?;
                // TODO: Implement struct constructor type checking
                Ok(TypedExpression {
                    value: ExpressionValue::StructConstructor {
                        identifier: identifier.clone(),
                        arguments: HashMap::new(),
                    },
                    ty: Typing::unknown(&expression.span),
                    span: expression.span,
                })
            }
            ExpressionValue::FieldAccess {
                parent_identifier,
                identifier,
            } => {
                let struct_type = environment
                    .lookup_variable(parent_identifier)
                    .cloned()
                    .ok_or_else(|| {
                        vec![error::undefined_type(
                            format!("type `{identifier}` is not defined"),
                            identifier,
                            expression.span,
                        )]
                    })?;

                // check if the struct type is actually a struct
                let struct_members = match &struct_type.value {
                    TypingValue::Struct(struct_value) => struct_value,
                    _ => todo!("field access on non-struct type: {struct_type:?}"),
                };

                let struct_member = struct_members
                    .iter()
                    .find(|member| member.name == *identifier)
                    .cloned()
                    .map(Ok)
                    .unwrap_or_else(|| {
                        Err(vec![error::undefined_field(
                            format!(
                                "field `{}` is not defined in the struct `{}`",
                                identifier, parent_identifier
                            ),
                            identifier,
                            &struct_type,
                            expression.span,
                        )])
                    })?
                    .clone();

                Ok(TypedExpression {
                    value: ExpressionValue::FieldAccess {
                        parent_identifier: parent_identifier.clone(),
                        identifier: identifier.clone(),
                    },
                    ty: struct_member.ty.clone().with_span(expression.span),
                    span: expression.span,
                })
            }
            ExpressionValue::Lambda {
                parameters,
                explicit_return_type,
                body,
            } => {
                let mut environment = environment.block();

                for parameter in parameters.iter() {
                    environment.declare_variable(&parameter.identifier, &parameter.ty);
                }

                let return_value = self.type_check_expression(body, &mut environment)?;

                if let Some(return_type) = explicit_return_type {
                    if !types_match(&return_value.ty, return_type, &environment)? {
                        self.report_error(error::new_mismatched_types(
                            "expected the return type to match",
                            &return_value.ty,
                            return_type,
                            format!(
                                "the lambda requires the return type to be {}, but {} was returned",
                                return_type, return_value.ty,
                            ),
                        ));
                    }
                }

                Ok(TypedExpression {
                    value: ExpressionValue::Lambda {
                        parameters: parameters.clone(),
                        explicit_return_type: explicit_return_type.clone(),
                        body: Box::new(return_value),
                    },
                    ty: Typing::unknown(&expression.span),
                    span: expression.span,
                })
            }
        }
    }

    fn type_check_statement(
        &mut self,
        statement: &Statement,
        environment: &mut Environment,
    ) -> Result<TypedStatement> {
        match &statement.value {
            StatementValue::Block(statements) => {
                // clone the outer environment for this block scope
                let mut environment = environment.block();

                let mut typed_statements = Vec::new();
                for statement in statements.iter() {
                    typed_statements.push(self.type_check_statement(statement, &mut environment)?);
                }
                let statements = typed_statements;

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
            StatementValue::Declaration(identifier, explicit_type, value) => {
                let value = self.type_check_expression(value, environment)?;

                if let Some(explicit_type) = explicit_type {
                    if !types_match(&value.ty, explicit_type, environment)? {
                        self.report_error(error::new_mismatched_types(
                            "expected the types to match",
                            &value.ty,
                            explicit_type,
                            format!("{} and {} do not match", value.ty, explicit_type),
                        ));
                    }
                }

                match &value.value {
                    ExpressionValue::Lambda {
                        parameters,
                        explicit_return_type,
                        body,
                    } => {
                        let signature = LambdaSignature {
                            span: statement.span,
                            parameters: parameters.clone(),
                            explicit_return_type: explicit_return_type.clone(),
                        };

                        let function = TypedFunction {
                            identifier: identifier.clone(),
                            signature: signature.clone(),
                            body: body.clone(),
                        };

                        environment.declare_function(identifier, &function)?;
                    }
                    _ => environment.declare_variable(identifier, &value.ty),
                };

                Ok(TypedStatement {
                    value: StatementValue::Declaration(
                        identifier.clone(),
                        explicit_type.clone(),
                        Box::new(value),
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
                let condition = self.type_check_expression(condition, &mut environment.block())?;
                let statement = self.type_check_statement(statement, &mut environment.block())?;

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
            StatementValue::TypeDeclaration(identifier, typing) => todo!(),
        }
    }

    fn declare_function(
        &self,
        identifier: &Identifier,
        signature: &LambdaSignature,
        environment: &mut Environment,
    ) -> Result<()> {
        let dummy = TypedExpression {
            value: ExpressionValue::Primitive(Primitive::Unit),
            ty: signature
                .explicit_return_type
                .as_ref()
                .map(|ty| (**ty).clone())
                .unwrap_or(Typing::unknown(&signature.span)),
            span: signature.span,
        };

        let placeholder = TypedFunction {
            identifier: identifier.clone(),
            signature: signature.clone(),
            body: Box::new(dummy),
        };

        environment.declare_function(identifier, &placeholder)?;

        Ok(())
    }

    fn declare_intrinsic_function(
        &self,
        identifier: &Identifier,
        signature: &IntrinsicSignature,
        environment: &mut Environment,
    ) -> Result<()> {
        let dummy = TypedExpression {
            value: ExpressionValue::Primitive(Primitive::Unit),
            ty: signature.return_type.clone(),
            span: signature.span,
        };

        let placeholder = TypedFunction {
            identifier: identifier.clone(),
            signature: LambdaSignature {
                span: signature.span,
                parameters: signature.parameters.clone(),
                explicit_return_type: Some(Box::new(signature.return_type.clone())),
            },
            body: Box::new(dummy),
        };

        environment.declare_function(identifier, &placeholder)?;

        Ok(())
    }

    fn type_check_function(
        &mut self,
        function: &Function,
        environment: &mut Environment,
    ) -> Result<TypedFunction> {
        for parameter in &function.signature.parameters {
            environment.declare_variable(&parameter.identifier, &parameter.ty);
        }

        let return_value = self.type_check_expression(&function.body, &mut environment.clone())?;

        if let Some(return_type) = &function.signature.explicit_return_type {
            if !types_match(&return_value.ty, return_type, environment)? {
                self.report_error(error::new_mismatched_types(
                    "expected the return type to match",
                    &return_value.ty,
                    return_type,
                    format!(
                        "the function `{}` requires the return type to be {}, but {} was returned",
                        function.identifier, return_type, return_value.ty,
                    ),
                ));
            }
        }

        // TODO: Add a warning that when using recursive functions, the return type must be explicitly set

        let declaration = TypedFunction {
            identifier: function.identifier.clone(),
            signature: function.signature.clone(),
            body: Box::new(return_value),
        };

        environment.update_function(&function.identifier, &declaration)?;

        Ok(declaration)
    }
}

fn types_match(a: &Typing, b: &Typing, environment: &Environment) -> Result<bool> {
    let mut errors = vec![];

    let a = a.unzip(environment);
    let b = b.unzip(environment);

    let a_matches = match type_matches(a, b.value.clone(), environment) {
        Ok(a) => a,
        Err(e) => {
            errors.extend(e.clone());
            false
        }
    };

    let b_matches = match type_matches(b, a.value.clone(), environment) {
        Ok(b) => b,
        Err(e) => {
            errors.extend(e.clone());
            false
        }
    };

    if errors.is_empty() {
        Ok(a_matches && b_matches)
    } else {
        Err(errors)
    }
}

fn type_matches(ty: &Typing, value: TypingValue, environment: &Environment) -> Result<bool> {
    let ty = ty.unzip(environment);
    let value = value.unzip(environment);

    if let TypingValue::Symbol(identifier) = &ty.value {
        return Err(vec![error::undefined_type(
            format!("type `{}` is not defined", identifier),
            identifier,
            ty.span,
        )]);
    }

    if ty.value == TypingValue::Unknown {
        return Ok(false);
    }

    Ok(ty.value == *value)
}

impl Typing {
    pub fn unzip<'a>(&'a self, environment: &'a Environment) -> &'a Typing {
        let unwrapped_ty = match &self.value {
            TypingValue::Symbol(identifier) => environment.lookup_type(identifier),
            _ => None,
        };

        if let Some(ty) = unwrapped_ty {
            ty
        } else {
            self
        }
    }
}

impl TypingValue {
    pub fn unzip<'a>(&'a self, environment: &'a Environment) -> &'a TypingValue {
        let unwrapped_ty = match &self {
            TypingValue::Symbol(identifier) => environment.lookup_type(identifier),
            _ => None,
        };

        if let Some(ty) = unwrapped_ty {
            &ty.value
        } else {
            self
        }
    }
}
