use std::{borrow::Cow, fmt::Display};

use miette::SourceSpan;

#[derive(Debug, Clone)]
pub enum Symbol<'de> {
    Statement(Statement<'de>),
    Expression(Expression<'de>),
}

#[derive(Debug, Clone)]
pub struct Statement<'de> {
    pub value: StatementValue<'de>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StatementValue<'de> {
    Block(Vec<Statement<'de>>),
    Expression(Expression<'de>),
    Assignment {
        name: Cow<'de, str>,
        value: Expression<'de>,
    },
    Struct {
        name: Cow<'de, str>,
        fields: Vec<StructMemberDeclaration<'de>>,
    },
    Enum {
        name: Cow<'de, str>,
        variants: Vec<EnumMemberDeclaration<'de>>,
    },
    Function {
        header: FunctionHeader<'de>,
        body: Expression<'de>,
    },
    Trait {
        name: Cow<'de, str>,
        functions: Vec<FunctionHeader<'de>>,
    },
    Return(Expression<'de>),
    Conditional {
        condition: Box<Expression<'de>>,
        truthy: Box<Statement<'de>>,
        falsy: Option<Box<Statement<'de>>>,
    },
}

#[derive(Debug, Clone)]
pub struct Expression<'de> {
    pub value: ExpressionValue<'de>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum ExpressionValue<'de> {
    Primitive(Primitive<'de>),
    Binary {
        operator: BinaryOperator,
        left: Box<Expression<'de>>,
        right: Box<Expression<'de>>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression<'de>>,
    },
    Group(Box<Expression<'de>>),
    Block {
        statements: Vec<Statement<'de>>,
        return_value: Box<Expression<'de>>,
    },
    Conditional {
        condition: Box<Expression<'de>>,
        truthy: Box<Expression<'de>>,
        falsy: Box<Expression<'de>>,
    },
    Call {
        callee: Box<Expression<'de>>,
        arguments: Vec<Expression<'de>>,
    },
}

#[derive(Debug, Clone)]
pub struct FunctionHeader<'de> {
    pub name: Cow<'de, str>,
    pub parameters: Vec<ParameterDeclaration<'de>>,
    pub explicit_return_type: Option<Type<'de>>,
}

#[derive(Debug, Clone)]
pub struct ParameterDeclaration<'de> {
    pub name: Cow<'de, str>,
    pub explicit_type: Type<'de>,
}

#[derive(Debug, Clone)]
pub struct StructMemberDeclaration<'de> {
    pub name: Cow<'de, str>,
    pub explicit_type: Type<'de>,
}

#[derive(Debug, Clone)]
pub struct EnumMemberDeclaration<'de> {
    pub name: Cow<'de, str>,
    pub value_type: Option<Type<'de>>,
}

#[derive(Debug, Clone)]
pub enum Primitive<'de> {
    Integer(i64),
    Decimal(f64),
    String(Cow<'de, str>),
    Identifier(Cow<'de, str>),
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

#[derive(Debug, Clone)]
pub enum UnaryOperator {
    Negate,
    Negative,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type<'de> {
    pub value: TypeValue<'de>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeValue<'de> {
    Unit,
    Boolean,
    Integer,
    Decimal,
    Character,
    String,
    Symbol(Cow<'de, str>),
    Collection(Box<Type<'de>>),
    Set(Box<Type<'de>>),
}

impl Display for Type<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.value {
            TypeValue::Unit => write!(f, "an unit"),
            TypeValue::Boolean => write!(f, "a boolean"),
            TypeValue::Integer => write!(f, "an integer"),
            TypeValue::Decimal => write!(f, "a decimal"),
            TypeValue::Character => write!(f, "a character"),
            TypeValue::String => write!(f, "a string"),
            TypeValue::Symbol(name) => write!(f, "{}", name),
            TypeValue::Collection(element) => write!(f, "[{}]", element),
            TypeValue::Set(element) => write!(f, "{{{}}}", element),
        }
    }
}

pub trait Spannable<'de>: Sized {
    type Value;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self;

    fn at_multiple(spans: Vec<impl Into<miette::SourceSpan>>, value: Self::Value) -> Self {
        let spans = spans.into_iter().map(|s| s.into()).collect::<Vec<_>>();

        let start = spans
            .iter()
            .min_by_key(|s| s.offset())
            .map(|s| s.offset())
            .unwrap_or(0);

        let end = spans
            .iter()
            .max_by_key(|s| s.offset() + s.len())
            .map(|s| s.offset() + s.len())
            .unwrap_or(0);

        let span = miette::SourceSpan::new(start.into(), end - start);

        Self::at(span, value)
    }
}

impl<'de> Spannable<'de> for Expression<'de> {
    type Value = ExpressionValue<'de>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'de> Spannable<'de> for Statement<'de> {
    type Value = StatementValue<'de>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}

impl<'de> Spannable<'de> for Type<'de> {
    type Value = TypeValue<'de>;

    fn at(span: miette::SourceSpan, value: Self::Value) -> Self {
        Self { value, span }
    }
}
