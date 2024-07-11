use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use crate::scanner::lexeme::Token;

#[derive(Debug, Clone, PartialEq)]
pub enum Symbol {
    Expression(Expression),
    Statement(Statement),
    Type(Type),
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
    FunctionCall(String, Vec<Expression>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Block(Vec<Statement>),
    Declaration(String, Option<Type>, Expression),
    Expression(Expression),
    Struct(String, HashSet<FieldSignature>),
    Enum(String, HashSet<EnumMember>),
    Function(Function),
    Return(Expression),
    Trait(String, HashSet<FunctionSignature>, HashSet<FieldSignature>),
    Implementation(String, String, HashSet<Function>),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub name: String,
    pub parameters: HashMap<String, Type>,
    pub return_type: Type,
}

impl Hash for FunctionSignature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSignature {
    pub name: String,
    pub typest: Type,
}

impl Hash for FieldSignature {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumMember {
    pub name: String,
    pub typest: Option<Type>,
}

impl Hash for EnumMember {
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
