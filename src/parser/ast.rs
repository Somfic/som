use std::collections::HashMap;

use crate::scanner::lexeme::Lexeme;

#[derive(Debug, Clone, PartialEq)]
pub enum Symbol {
    Expression(Expression),
    Statement(Statement),
    Type(Type),
    Unknown(Lexeme),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Number(f64),
    String(String),
    Identifier(String),
    Unary(UnaryOperation, Box<Expression>),
    Binary(Box<Expression>, BinaryOperation, Box<Expression>),
    Grouping(Box<Expression>),
    Assignment(Box<Expression>, Box<Expression>),
    StructInitializer(String, HashMap<String, Expression>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Block(Vec<Statement>),
    Declaration(String, Option<Type>, Expression),
    Expression(Expression),
    Struct(String, HashMap<String, Type>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOperation {
    Negate,
    Inverse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOperation {
    Plus,
    Minus,
    Times,
    Divide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Symbol(String),
    Array(Box<Type>),
    Tuple(HashMap<String, Type>),
}
