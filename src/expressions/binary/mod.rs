pub mod add;
pub mod divide;
pub mod multiply;
pub mod subtract;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryExpression<Expression> {
    pub left: Box<Expression>,
    pub operator: BinaryOperator,
    pub right: Box<Expression>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}
