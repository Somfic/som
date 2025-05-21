pub mod boolean;
pub mod integer;
pub mod unit;

#[derive(Debug, Clone, PartialEq)]
pub enum PrimaryExpression {
    Unit,
    Integer(i64),
    Boolean(bool),
}
