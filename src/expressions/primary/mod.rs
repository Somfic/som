pub mod boolean;
pub mod integer;
pub mod string;
pub mod unit;

#[derive(Debug, Clone)]
pub enum PrimaryExpression {
    Unit,
    I32(i32),
    I64(i64),
    Boolean(bool),
    String(Box<str>),
}
