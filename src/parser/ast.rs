use std::collections::{HashMap, HashSet};

use crate::scanner::lexeme::Token;

#[derive(Debug, Clone, PartialEq)]
pub enum Symbol<'a> {
    Expression(Expression),
    Statement(Statement),
    Type(Type),
    Unknown(Token<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Number(f64),
    String(String),
    Identifier(String),
    Boolean(bool),
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
    Enum(String, HashMap<String, Option<Type>>),
    Function(String, HashMap<String, Type>, Type, Box<Statement>),
    Return(Expression),
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
    Void,
    Symbol(String),
    Array(Box<Type>),
    Tuple(HashMap<String, Type>),
}
