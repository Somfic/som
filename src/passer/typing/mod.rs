use core::net;

use super::{Passer, PasserResult};
use crate::parser::{
    ast::{
        Expression, ExpressionValue, Spannable, Statement, StatementValue, Symbol, Type, TypeValue,
    },
    expression,
};
use miette::{Error, LabeledSpan, Report, Result};

pub struct TypingPasser;

impl Passer for TypingPasser {
    fn pass(ast: &Symbol<'_>) -> Result<PasserResult> {
        fn check_expression(expression: &Expression<'_>) -> Result<PasserResult> {
            let mut critical = vec![];

            let types = expression.possible_types();

            let distinct_types = types.clone().into_iter().fold(vec![], |mut acc, ty| {
                if !acc.iter().any(|t: &Type<'_>| t.value == ty.value) {
                    acc.push(ty);
                }
                acc
            });

            if distinct_types.is_empty() || distinct_types.len() == 1 {
                return Ok(PasserResult::default());
            }

            let labels = types
                .iter()
                .map(|ty| LabeledSpan::at(ty.span, format!("{}", ty)))
                .collect::<Vec<_>>();

            // critical.push(miette::miette! {
            //     labels = labels,
            //     help = "expression has multiple possible types",
            //     "{:?} has multiple possible types", expression.value
            // });

            Ok(PasserResult {
                critical,
                non_critical: vec![],
            })
        }

        fn check_statement(statement: &Statement<'_>) -> Result<PasserResult> {
            let mut critical = vec![];

            let types = statement.possible_types();

            let distinct_types = types.clone().into_iter().fold(vec![], |mut acc, ty| {
                if !acc.iter().any(|t: &Type<'_>| t.value == ty.value) {
                    acc.push(ty);
                }
                acc
            });

            if distinct_types.is_empty() || distinct_types.len() == 1 {
                return Ok(PasserResult::default());
            }

            let labels = types
                .iter()
                .map(|ty| LabeledSpan::at(ty.span, format!("{}", ty)))
                .collect::<Vec<_>>();

            critical.push(miette::miette! {
                labels = labels,
                help = "statement has multiple possible types",
                "{:?} has multiple possible types", statement.value
            });

            Ok(PasserResult {
                critical,
                non_critical: vec![],
            })
        }

        walk(ast, check_statement, check_expression)
    }
}

pub fn walk<'de>(
    symbol: &Symbol<'de>,
    statement_fn: fn(&Statement<'de>) -> Result<PasserResult>,
    expression_fn: fn(&Expression<'de>) -> Result<PasserResult>,
) -> Result<PasserResult> {
    match symbol {
        Symbol::Statement(statement) => walk_statement(statement, statement_fn, expression_fn),
        Symbol::Expression(expression) => walk_expression(expression, statement_fn, expression_fn),
    }
}

pub fn walk_statement<'de>(
    statement: &Statement<'de>,
    statement_fn: fn(&Statement<'de>) -> Result<PasserResult>,
    expression_fn: fn(&Expression<'de>) -> Result<PasserResult>,
) -> Result<PasserResult> {
    let mut result = statement_fn(statement)?;

    match &statement.value {
        StatementValue::Block(statements) => {
            for statement in statements {
                result = result.combine(walk_statement(statement, statement_fn, expression_fn)?);
            }
        }
        StatementValue::Expression(expression) => {
            result = result.combine(walk_expression(expression, statement_fn, expression_fn)?);
        }
        StatementValue::Assignment { name, value } => {
            result = result.combine(walk_expression(value, statement_fn, expression_fn)?);
        }
        StatementValue::Struct { name, fields } => {}
        StatementValue::Enum { name, variants } => {}
        StatementValue::Function { header, body } => {
            result = result.combine(walk_expression(body, statement_fn, expression_fn)?);
        }
        StatementValue::Trait { name, functions } => {}
        StatementValue::Return(expression) => {
            result = result.combine(walk_expression(expression, statement_fn, expression_fn)?);
        }
        StatementValue::Conditional {
            condition,
            truthy,
            falsy,
        } => {
            result = result.combine(walk_expression(condition, statement_fn, expression_fn)?);
            result = result.combine(walk_statement(truthy, statement_fn, expression_fn)?);
            if let Some(falsy) = falsy {
                result = result.combine(walk_statement(falsy, statement_fn, expression_fn)?);
            }
        }
    }

    Ok(result)
}

pub fn walk_expression<'de>(
    expression: &Expression<'de>,
    statement_fn: fn(&Statement<'de>) -> Result<PasserResult>,
    expression_fn: fn(&Expression<'de>) -> Result<PasserResult>,
) -> Result<PasserResult> {
    let mut result = expression_fn(expression)?;

    match &expression.value {
        ExpressionValue::Binary {
            operator,
            left,
            right,
        } => {
            result = result.combine(walk_expression(left, statement_fn, expression_fn)?);
            result = result.combine(walk_expression(right, statement_fn, expression_fn)?);
        }
        ExpressionValue::Unary { operator, operand } => {
            result = result.combine(walk_expression(operand, statement_fn, expression_fn)?);
        }
        ExpressionValue::Group(expression) => {
            result = result.combine(walk_expression(expression, statement_fn, expression_fn)?);
        }
        ExpressionValue::Block {
            statements,
            return_value,
        } => {
            for statement in statements {
                result = result.combine(walk_statement(statement, statement_fn, expression_fn)?);
            }
            result = result.combine(walk_expression(return_value, statement_fn, expression_fn)?);
        }
        ExpressionValue::Conditional {
            condition,
            truthy,
            falsy,
        } => {
            result = result.combine(walk_expression(condition, statement_fn, expression_fn)?);
            result = result.combine(walk_expression(truthy, statement_fn, expression_fn)?);
            result = result.combine(walk_expression(falsy, statement_fn, expression_fn)?);
        }
        ExpressionValue::Call { callee, arguments } => {
            result = result.combine(walk_expression(callee, statement_fn, expression_fn)?);
            for argument in arguments {
                result = result.combine(walk_expression(argument, statement_fn, expression_fn)?);
            }
        }
        ExpressionValue::Primitive(_) => {}
    }

    Ok(result)
}

pub trait Typing<'de> {
    fn possible_types(&self) -> Vec<Type<'de>>;
}

impl<'de> Typing<'de> for Expression<'de> {
    fn possible_types(&self) -> Vec<Type<'de>> {
        match &self.value {
            ExpressionValue::Primitive(primitive) => vec![match primitive {
                crate::parser::ast::Primitive::Integer(_) => {
                    Type::at(self.span, TypeValue::Integer)
                }
                crate::parser::ast::Primitive::Decimal(_) => {
                    Type::at(self.span, TypeValue::Decimal)
                }
                crate::parser::ast::Primitive::String(_) => Type::at(self.span, TypeValue::String),
                crate::parser::ast::Primitive::Identifier(value) => {
                    Type::at(self.span, TypeValue::Symbol(value.clone()))
                }
                crate::parser::ast::Primitive::Character(_) => {
                    Type::at(self.span, TypeValue::Character)
                }
                crate::parser::ast::Primitive::Boolean(_) => {
                    Type::at(self.span, TypeValue::Boolean)
                }
                crate::parser::ast::Primitive::Unit => Type::at(self.span, TypeValue::Unit),
            }],
            ExpressionValue::Binary {
                operator: _,
                left,
                right,
            } => {
                let mut types = left.possible_types();
                types.extend(right.possible_types());
                types
            }
            ExpressionValue::Unary {
                operator: _,
                operand,
            } => operand.possible_types(),
            ExpressionValue::Group(expression) => expression.possible_types(),
            ExpressionValue::Block {
                statements: _,
                return_value,
            } => return_value.possible_types(),
            ExpressionValue::Conditional {
                condition: _,
                truthy,
                falsy,
            } => {
                let mut types = truthy.possible_types();
                types.extend(falsy.possible_types());
                types
            }
            ExpressionValue::Call {
                callee,
                arguments: _,
            } => callee.possible_types(),
        }
    }
}

impl<'de> Typing<'de> for Statement<'de> {
    fn possible_types(&self) -> Vec<Type<'de>> {
        match &self.value {
            StatementValue::Block(statements) => vec![],
            StatementValue::Expression(expression) => expression.possible_types(),
            StatementValue::Assignment { name: _, value } => value.possible_types(),
            StatementValue::Struct { name, fields } => {
                vec![Type::at(self.span, TypeValue::Symbol(name.clone()))]
            }
            StatementValue::Enum { name, variants } => {
                vec![Type::at(self.span, TypeValue::Symbol(name.clone()))]
            }
            StatementValue::Function { header, body } => {
                let mut types = body.possible_types();
                if let Some(explicit_return_type) = &header.explicit_return_type {
                    types.push(explicit_return_type.clone());
                }
                types
            }
            StatementValue::Trait { name, functions } => {
                vec![Type::at(self.span, TypeValue::Symbol(name.clone()))]
            }
            StatementValue::Return(expression) => expression.possible_types(),
            StatementValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let mut types = truthy.possible_types();
                if let Some(falsy) = falsy {
                    types.extend(falsy.possible_types());
                }
                types
            }
        }
    }
}
