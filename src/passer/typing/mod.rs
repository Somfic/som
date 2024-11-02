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
            Ok(PasserResult::default())
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
                "{} has multiple possible types", statement.value
            });

            Ok(PasserResult {
                critical,
                non_critical: vec![],
            })
        }

        walk(ast, check_statement, check_expression)
    }
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
