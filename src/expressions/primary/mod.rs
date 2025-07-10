pub mod boolean;
pub mod integer;
pub mod unit;

#[derive(Debug, Clone)]
pub enum PrimaryExpression {
    Unit,
    I32(i32),
    I64(i64),
    Boolean(bool),
}
