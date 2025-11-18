use crate::{ast::Statement, lexer::Identifier, Phase, Span, Type};
use std::fmt::{write, Display};

#[derive(Debug)]
pub enum Expression<P: Phase> {
    Primary(Primary<P>),
    Unary(Unary<P>),
    Binary(Binary<P>),
    Group(Group<P>),
    Block(Block<P>),
    Ternary(Ternary<P>),
    Lambda(Lambda<P>),
    Call(Call<P>),
}

impl<P: Phase> Expression<P> {
    pub fn span(&self) -> &Span {
        match self {
            Expression::Primary(p) => &p.span,
            Expression::Binary(b) => &b.span,
            Expression::Unary(u) => &u.span,
            Expression::Group(g) => &g.span,
            Expression::Block(b) => &b.span,
            Expression::Ternary(t) => &t.span,
            Expression::Lambda(l) => &l.span,
            Expression::Call(c) => &c.span,
        }
    }

    pub fn ty(&self) -> &P::TypeInfo {
        match self {
            Expression::Primary(p) => &p.ty,
            Expression::Binary(b) => &b.ty,
            Expression::Unary(u) => &u.ty,
            Expression::Group(g) => &g.ty,
            Expression::Block(b) => &b.ty,
            Expression::Ternary(t) => &t.ty,
            Expression::Lambda(l) => &l.ty,
            Expression::Call(c) => &c.ty,
        }
    }
}

impl<P: Phase> Display for Expression<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expression::Primary(p) => write!(f, "{}", p),
            Expression::Unary(u) => write!(f, "{}", u),
            Expression::Binary(b) => write!(f, "{}", b),
            Expression::Group(g) => write!(f, "{}", g.expr),
            Expression::Block(b) => write!(f, "a block"),
            Expression::Ternary(t) => write!(f, "a ternary"),
            Expression::Lambda(l) => write!(f, "a lambda"),
            Expression::Call(c) => write!(f, "a function call"),
        }
    }
}

#[derive(Debug)]
pub struct Primary<P: Phase> {
    pub kind: PrimaryKind,
    pub span: Span,
    pub ty: P::TypeInfo,
}

#[derive(Debug)]
pub enum PrimaryKind {
    Boolean(bool),
    I32(i32),
    I64(i64),
    Decimal(f64),
    String(Box<str>),
    Character(char),
    Identifier(Identifier),
}

impl<P: Phase> Display for Primary<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            PrimaryKind::Boolean(b) => write!(f, "a boolean"),
            PrimaryKind::I32(i) => write!(f, "a number"),
            PrimaryKind::I64(i) => write!(f, "a number"),
            PrimaryKind::Decimal(d) => write!(f, "a number"),
            PrimaryKind::String(s) => write!(f, "a string"),
            PrimaryKind::Character(c) => write!(f, "a character"),
            PrimaryKind::Identifier(id) => write!(f, "an identifier"),
        }
    }
}

#[derive(Debug)]
pub enum UnaryOperation {
    Negate,
}

#[derive(Debug)]
pub struct Unary<P: Phase> {
    pub op: UnaryOperation,
    pub value: Box<Expression<P>>,
    pub span: Span,
    pub ty: P::TypeInfo,
}

impl<P: Phase> Display for Unary<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.op {
            UnaryOperation::Negate => write!(f, "a negation"),
        }
    }
}

#[derive(Debug)]
pub struct Binary<P: Phase> {
    pub lhs: Box<Expression<P>>,
    pub rhs: Box<Expression<P>>,
    pub op: BinaryOperation,
    pub ty: P::TypeInfo,
    pub span: Span,
}

impl<P: Phase> Display for Binary<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.op {
            BinaryOperation::Add => write!(f, "an addition"),
            BinaryOperation::Subtract => write!(f, "a subtraction"),
            BinaryOperation::Multiply => write!(f, "a multiplication"),
            BinaryOperation::Divide => write!(f, "a division"),
            BinaryOperation::LessThan => write!(f, "a less-than comparison"),
            BinaryOperation::LessThanOrEqual => write!(f, "a less-than-or-equal comparison"),
            BinaryOperation::GreaterThan => write!(f, "a greater-than comparison"),
            BinaryOperation::GreaterThanOrEqual => write!(f, "a greater-than-or-equal comparison"),
            BinaryOperation::Equality => write!(f, "an equality comparison"),
            BinaryOperation::Inequality => write!(f, "an inequality comparison"),
        }
    }
}

#[derive(Debug)]
pub enum BinaryOperation {
    Add,
    Subtract,
    Multiply,
    Divide,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equality,
    Inequality,
}

#[derive(Debug)]
pub struct Group<P: Phase> {
    pub expr: Box<Expression<P>>,
    pub span: Span,
    pub ty: P::TypeInfo,
}

#[derive(Debug)]
pub struct Block<P: Phase> {
    pub statements: Vec<Statement<P>>,
    pub expression: Option<Box<Expression<P>>>,
    pub span: Span,
    pub ty: P::TypeInfo,
}

#[derive(Debug)]
pub struct Ternary<P: Phase> {
    pub condition: Box<Expression<P>>,
    pub truthy: Box<Expression<P>>,
    pub falsy: Box<Expression<P>>,
    pub span: Span,
    pub ty: P::TypeInfo,
}

impl<P: Phase> Display for Ternary<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a ternary expression")
    }
}

#[derive(Debug)]
pub struct Lambda<P: Phase> {
    pub id: usize,
    pub parameters: Vec<Parameter>,
    pub explicit_return_ty: Option<Type>,
    pub body: Box<Expression<P>>,
    pub span: Span,
    pub ty: P::TypeInfo,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: Type,
}

#[derive(Debug)]
pub struct Call<P: Phase> {
    pub callee: Box<Expression<P>>,
    pub arguments: Vec<Expression<P>>,
    pub span: Span,
    pub ty: P::TypeInfo,
}
