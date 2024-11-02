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

impl Display for StatementValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StatementValue::Block(vec) => write!(f, "a block of {} statements", vec.len()),
            StatementValue::Expression(expression) => write!(f, "{}", expression),
            StatementValue::Assignment { name, value } => {
                write!(f, "`{}` assignment with {}", name, value)
            }
            StatementValue::Struct { name, fields } => write!(f, "`{}` struct", name),
            StatementValue::Enum { name, variants } => write!(f, "`{}` enum", name),
            StatementValue::Function { header, body } => write!(f, "`{}` function", header.name),
            StatementValue::Trait { name, functions } => write!(f, "`{}` trait", name),
            StatementValue::Return(expression) => write!(f, "returning {}", expression),
            StatementValue::Conditional {
                condition,
                truthy,
                falsy,
            } => write!(f, "conditional statement"),
        }
    }
}

impl Display for Statement<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Display for ExpressionValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpressionValue::Primitive(primitive) => write!(f, "{}", primitive),
            ExpressionValue::Binary {
                operator,
                left,
                right,
            } => write!(f, "{} expression", operator),
            ExpressionValue::Unary { operator, operand } => write!(f, "{} expression", operator),
            ExpressionValue::Group(expression) => write!(f, "grouped expression"),
            ExpressionValue::Block {
                statements,
                return_value,
            } => write!(f, "block expression"),
            ExpressionValue::Conditional {
                condition,
                truthy,
                falsy,
            } => write!(f, "conditional expression"),
            ExpressionValue::Call { callee, arguments } => write!(f, "calling {}", callee.value),
        }
    }
}

impl Display for Primitive<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Primitive::Integer(value) => write!(f, "{}", value),
            Primitive::Decimal(value) => write!(f, "{}", value),
            Primitive::String(value) => write!(f, "{}", value),
            Primitive::Identifier(value) => write!(f, "{}", value),
            Primitive::Character(value) => write!(f, "{}", value),
            Primitive::Boolean(value) => write!(f, "{}", value),
            Primitive::Unit => write!(f, "nothing"),
        }
    }
}

impl Display for Expression<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Display for BinaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryOperator::Add => write!(f, "addition"),
            BinaryOperator::Subtract => write!(f, "subtraction"),
            BinaryOperator::Multiply => write!(f, "multiplication"),
            BinaryOperator::Divide => write!(f, "division"),
            BinaryOperator::Modulo => write!(f, "modulo"),
            BinaryOperator::Equality => write!(f, "equality"),
            BinaryOperator::Inequality => write!(f, "inequality"),
            BinaryOperator::LessThan => write!(f, "less than"),
            BinaryOperator::LessThanOrEqual => write!(f, "less than or equal"),
            BinaryOperator::GreaterThan => write!(f, "greater than"),
            BinaryOperator::GreaterThanOrEqual => write!(f, "greater than or equal"),
            BinaryOperator::And => write!(f, "and"),
            BinaryOperator::Or => write!(f, "or"),
        }
    }
}

impl Display for UnaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnaryOperator::Negate => write!(f, "negation"),
            UnaryOperator::Negative => write!(f, "negative"),
        }
    }
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
            TypeValue::Unit => write!(f, "nothing"),
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
