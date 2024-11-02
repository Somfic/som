use std::collections::HashSet;

use crate::parser::ast::Symbol;
use miette::{Report, Result, SourceSpan};

pub mod typing;

pub trait Passer {
    fn pass(ast: &Symbol<'_>) -> Result<PasserResult>;
}

#[derive(Default, Debug)]
pub struct PasserResult {
    pub non_critical: Vec<Report>,
    pub critical: Vec<Report>,
}

impl PasserResult {
    pub fn combine(mut self, other: PasserResult) -> Self {
        self.non_critical.extend(other.non_critical);
        self.critical.extend(other.critical);

        self
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

fn walk_statement<'de>(
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

fn walk_expression<'de>(
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
