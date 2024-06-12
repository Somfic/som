use crate::scanner::lexeme::Lexeme;

#[derive(Debug, Clone, PartialEq)]
pub enum Symbol {
    Expression(Expression),
    Statement(Statement),
    Unknown(Lexeme),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Number(f64),
    String(String),
    Identifier(String),
    Binary(Box<Expression>, BinaryOperation, Box<Expression>),
    Grouping(Box<Expression>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Block(Vec<Statement>),
    Declaration(String, Expression),
    Expression(Expression),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOperation {
    Plus,
    Minus,
    Times,
    Divide,
}
