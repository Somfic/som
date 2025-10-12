pub mod add;
pub mod divide;
pub mod equals;
pub mod greater_than;
pub mod greater_than_or_equal;
pub mod less_than;
pub mod multiply;
pub mod subtract;

#[derive(Debug, Clone)]
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
    LessThan,
    GreaterThan,
    GreaterThanOrEqual,
    Equals,
}
