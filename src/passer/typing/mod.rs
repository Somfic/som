use super::{Passer, PasserResult};
use crate::parser::{
    ast::{Expression, ExpressionValue, Spannable, Statement, StatementValue, Symbol, Type, TypeValue},
    expression,
};
use miette::{Error, LabeledSpan, Report, Result};

pub struct TypingPasser;

impl Passer for TypingPasser {
    fn pass(ast: &Symbol<'_>) -> Result<PasserResult> {
       todo!()
    }
}


pub fn walk_statement<'de>(statement: &Statement<'de>, statement_fn: fn(&Statement<'de>), expression_fn: fn(&Expression<'de>)) {
    match statement {
        Statement::Block(statements) => statements.iter().for_each(|statement| {
            walk_statement(statement, statement_fn, expression_fn);
        }),
        Statement::Expression(expression) => walk_expression(statement_fn, expression_fn),
        Statement::Assignment { name, value } => walk_expression(expression, expression_fn),
        Statement::Struct { name, fields } => {},
        Statement::Enum { name, variants } => {},
        Statement::Function {
            header,
            body,
            explicit_return_type,
        } => ,
        Statement::Trait { name, functions } => todo!(),
        Statement::Return(expression) => todo!(),
        Statement::Conditional {
            condition,
            truthy,
            falsy,
        } => todo!(),
    }
}

pub fn walk_expression<'de, T>(expression: Expression<'de>, statement_fn: fn(&Statement<'de>, expression_fn: fn(&Expression<'de>))) {
    todo!()
}

pub trait Typing {
    fn possible_types(&self) -> Vec<(Type, miette::SourceSpan)>;
}

impl Typing for Expression<'_> {
    fn possible_types(&self) -> Vec<(Type, miette::SourceSpan)> {
        match &self.value {
            ExpressionValue::Primitive(primitive) => vec![match primitive {
                crate::parser::ast::Primitive::Integer(_) => Type::at(self.span, TypeValue::Integer),
                crate::parser::ast::Primitive::Decimal(_) => (Type::Decimal, self.span),
                crate::parser::ast::Primitive::String(_) => (Type::String, self.span),
                crate::parser::ast::Primitive::Identifier(value) => {
                    (Type::Symbol(value.clone()), self.span)
                }
                crate::parser::ast::Primitive::Character(_) => (Type::Character, self.span),
                crate::parser::ast::Primitive::Boolean(_) => (Type::Boolean, self.span),
                crate::parser::ast::Primitive::Unit => (Type::Unit, self.span),
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

impl Typing for Statement<'_> {
    fn possible_types(&self) -> Vec<(Type, miette::SourceSpan)> {
        match &self.value {
            StatementValue::Block(statements) => vec![],
            StatementValue::Expression(expression) => expression.possible_types(),
            StatementValue::Assignment { name: _, value } => value.possible_types(),
            StatementValue::Struct { name, fields } => vec![(Type::Symbol(name.clone()), self.span)],
            StatementValue::Enum { name, variants } => vec![(Type::Symbol(name.clone()), self.span)],
            StatementValue::Function {
                header: _,
                body,
               
            } => {
                let mut types = body.possible_types();
                if let Some(explicit_return_type) = explicit_return_type {
                    types.push((explicit_return_type,));
                }
                types
            }
            StatementValue::Trait { name, functions } => vec![(Type::Symbol(name.clone()), self.span)],
            StatementValue::Return(expression) => expression.possible_types(),
            StatementValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let mut types = condition.possible_types();
                types.extend(truthy.possible_types());
                if let Some(falsy) = falsy {
                    types.extend(falsy.possible_types());
                }
                types
            }
        }
    }
}
