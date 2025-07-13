pub mod negative;

#[derive(Debug, Clone)]
pub struct UnaryExpression<Expression> {
    pub operand: Box<Expression>,
    pub operator: UnaryOperator,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Negative,
    Negate,
}
