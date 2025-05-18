pub mod integer;

#[derive(Debug, Clone, PartialEq)]
pub enum PrimaryExpression {
    Integer(i64),
}
