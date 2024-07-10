use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

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
    Function(Function),
    Return(Expression),
    Trait(String, HashSet<FunctionSignature>),
    Implementation(String, Type, HashSet<Function>),
}

#[derive(Debug, Clone)]
pub struct Function {
    pub signature: FunctionSignature,
    pub body: Box<Statement>,
}

impl PartialEq for Function {
    fn eq(&self, other: &Self) -> bool {
        self.signature == other.signature
    }
}

impl Eq for Function {}

impl Hash for Function {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.signature.hash(state);
    }
}

#[derive(Debug, Clone, Eq)]
pub struct FunctionSignature {
    pub name: String,
    pub parameters: HashMap<String, Type>,
    pub return_type: Type,
}

impl PartialEq for FunctionSignature {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Hash for FunctionSignature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
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
