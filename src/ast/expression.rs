use std::borrow::Cow;

use super::Type;

#[derive(Debug, Clone)]
pub struct Expression<'ast> {
    pub value: ExpressionValue<'ast, Expression<'ast>>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub struct TypedExpression<'ast> {
    pub value: ExpressionValue<'ast, Expression<'ast>>,
    pub span: miette::SourceSpan,
    pub ty: Type<'ast>,
}

#[derive(Debug, Clone)]
pub enum ExpressionValue<'ast, Expression> {
    Primitive(Primitive<'ast>),
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
}

#[derive(Debug, Clone)]
pub enum Primitive<'ast> {
    Integer(i64),
    Decimal(f64),
    String(Cow<'ast, str>),
    Identifier(Cow<'ast, str>),
    Character(char),
    Boolean(bool),
    Unit,
}

#[derive(Debug, Clone)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equality,
    Inequality,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    And,
    Or,
}
