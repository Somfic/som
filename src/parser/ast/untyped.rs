use super::Type;
use std::{borrow::Cow, fmt::Display};

#[derive(Debug, Clone)]
pub enum Symbol<'ast> {
    Statement(Statement<'ast>),
    Expression(Expression<'ast>),
}

#[derive(Debug, Clone)]
pub struct Statement<'ast> {
    pub value: StatementValue<'ast>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StatementValue<'ast> {
    Block(Vec<Statement<'ast>>),
    Expression(Expression<'ast>),
    Assignment {
        name: Cow<'ast, str>,
        value: Expression<'ast>,
    },
    Struct {
        name: Cow<'ast, str>,
        fields: Vec<StructMemberDeclaration<'ast>>,
    },
    Enum {
        name: Cow<'ast, str>,
        variants: Vec<EnumMemberDeclaration<'ast>>,
    },
    Function {
        header: FunctionHeader<'ast>,
        body: Expression<'ast>,
    },
    Trait {
        name: Cow<'ast, str>,
        functions: Vec<FunctionHeader<'ast>>,
    },
    Return(Expression<'ast>),
    Conditional {
        condition: Box<Expression<'ast>>,
        truthy: Box<Statement<'ast>>,
        falsy: Option<Box<Statement<'ast>>>,
    },
    TypeAlias {
        name: std::borrow::Cow<'ast, str>,
        explicit_type: crate::parser::ast::Type<'ast>,
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
            StatementValue::Struct { name, fields: _ } => write!(f, "`{}` struct", name),
            StatementValue::Enum { name, variants: _ } => write!(f, "`{}` enum", name),
            StatementValue::Function { header, body: _ } => write!(f, "`{}` function", header.name),
            StatementValue::Trait { name, functions: _ } => write!(f, "`{}` trait", name),
            StatementValue::Return(expression) => write!(f, "returning {}", expression),
            StatementValue::Conditional {
                condition: _,
                truthy: _,
                falsy: _,
            } => write!(f, "conditional statement"),
            StatementValue::TypeAlias {
                name,
                explicit_type,
            } => {
                write!(f, "`{}` type alias with type {}", name, explicit_type)
            }
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
                left: _,
                right: _,
            } => write!(f, "{} expression", operator),
            ExpressionValue::Unary {
                operator,
                operand: _,
            } => write!(f, "{} expression", operator),
            ExpressionValue::Group(_expression) => write!(f, "grouped expression"),
            ExpressionValue::Block {
                statements: _,
                return_value: _,
            } => write!(f, "block expression"),
            ExpressionValue::Conditional {
                condition: _,
                truthy: _,
                falsy: _,
            } => write!(f, "conditional expression"),
            ExpressionValue::Call {
                callee,
                arguments: _,
            } => write!(f, "calling {}", callee.value),
            ExpressionValue::Lambda(_) => write!(f, "lambda expression"),
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
            BinaryOperator::LessThan => write!(f, "less than comparison"),
            BinaryOperator::LessThanOrEqual => write!(f, "less than or equal comparison"),
            BinaryOperator::GreaterThan => write!(f, "greater than comparison"),
            BinaryOperator::GreaterThanOrEqual => write!(f, "greater than or equal comparison"),
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
pub struct Expression<'ast> {
    pub value: ExpressionValue<'ast>,
    pub span: miette::SourceSpan,
}

impl Expression<'_> {
    pub fn label(&self, label: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, label)
    }
}

impl Statement<'_> {
    pub fn label(&self, label: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, label)
    }
}

#[derive(Debug, Clone)]
pub enum ExpressionValue<'ast> {
    Primitive(Primitive<'ast>),
    Binary {
        operator: BinaryOperator,
        left: Box<Expression<'ast>>,
        right: Box<Expression<'ast>>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression<'ast>>,
    },
    Group(Box<Expression<'ast>>),
    Block {
        statements: Vec<Statement<'ast>>,
        return_value: Box<Expression<'ast>>,
    },
    Conditional {
        condition: Box<Expression<'ast>>,
        truthy: Box<Expression<'ast>>,
        falsy: Box<Expression<'ast>>,
    },
    Call {
        callee: Box<Expression<'ast>>,
        arguments: Vec<Expression<'ast>>,
    },
    Lambda(Lambda<'ast>),
}

#[derive(Debug, Clone)]
pub struct FunctionHeader<'ast> {
    pub name: Cow<'ast, str>,
    pub parameters: Vec<ParameterDeclaration<'ast>>,
    pub explicit_return_type: Option<Type<'ast>>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Lambda<'ast> {
    pub parameters: Vec<ParameterDeclaration<'ast>>,
    pub body: Box<Expression<'ast>>,
}

#[derive(Debug, Clone)]
pub struct ParameterDeclaration<'ast> {
    pub name: Cow<'ast, str>,
    pub explicit_type: Type<'ast>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub struct StructMemberDeclaration<'ast> {
    pub name: Cow<'ast, str>,
    pub explicit_type: Type<'ast>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub struct EnumMemberDeclaration<'ast> {
    pub name: Cow<'ast, str>,
    pub value_type: Option<Type<'ast>>,
    pub span: miette::SourceSpan,
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
impl BinaryOperator {
    pub fn is_comparison(&self) -> bool {
        matches!(
            self,
            BinaryOperator::Equality
                | BinaryOperator::Inequality
                | BinaryOperator::LessThan
                | BinaryOperator::LessThanOrEqual
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterThanOrEqual
        )
    }
}

#[derive(Debug, Clone)]
pub enum UnaryOperator {
    Negate,
    Negative,
}
