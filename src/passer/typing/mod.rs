use super::{Passer, PasserResult};
use crate::parser::ast::{Expression, Statement, Symbol, Type};
use miette::{Result, Severity};

pub struct TypingPasser;

impl Passer for TypingPasser {
    fn pass(ast: &Symbol<'_>) -> Result<PasserResult> {
        let mut critical = vec![];
        let mut non_critical = vec![];

        Ok(PasserResult {
            critical,
            non_critical,
        })
    }
}

pub trait Typing {
    fn possible_types(&self) -> Vec<Type>;
}

impl Typing for Expression<'_> {
    fn possible_types(&self) -> Vec<Type> {
        match self {
            Expression::Primitive(primitive) => vec![match primitive {
                crate::parser::ast::Primitive::Integer(_) => Type::Integer,
                crate::parser::ast::Primitive::Decimal(_) => Type::Decimal,
                crate::parser::ast::Primitive::String(_) => Type::String,
                crate::parser::ast::Primitive::Identifier(value) => Type::Symbol(value.clone()),
                crate::parser::ast::Primitive::Character(_) => Type::Character,
                crate::parser::ast::Primitive::Boolean(_) => Type::Boolean,
                crate::parser::ast::Primitive::Unit => Type::Unit,
            }],
            Expression::Binary {
                operator,
                left,
                right,
            } => {
                let mut types = left.possible_types();
                types.extend(right.possible_types());
                types
            }
            Expression::Unary { operator, operand } => operand.possible_types(),
            Expression::Group(expression) => expression.possible_types(),
            Expression::Block {
                statements,
                return_value,
            } => return_value.possible_types(),
            Expression::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let mut types = truthy.possible_types();
                types.extend(falsy.possible_types());
                types
            }
            Expression::Call { callee, arguments } => callee.possible_types(),
        }
    }
}
