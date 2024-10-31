use std::borrow::Cow;

#[derive(Debug, Clone)]
pub enum Symbol<'de> {
    Statement(Statement<'de>),
    Expression(Expression<'de>),
}

#[derive(Debug, Clone)]
pub enum Statement<'de> {
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
        explicit_return_type: Option<Type<'de>>,
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
pub enum Expression<'de> {
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
pub enum Type<'de> {
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
