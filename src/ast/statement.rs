use super::{Type, TypedExpression};
use std::{
    borrow::Cow,
    fmt::{Display, Formatter},
};

pub type TypedStatement<'ast> = Statement<'ast, TypedExpression<'ast>>;

#[derive(Debug, Clone)]
pub struct Statement<'ast, Expression> {
    pub value: StatementValue<'ast, Expression>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StatementValue<'ast, Expression> {
    Block(Vec<Statement<'ast, Expression>>),
    Expression(Expression),
    Assignment {
        name: Cow<'ast, str>,
        value: Expression,
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
        body: Expression,
    },
    Trait {
        name: Cow<'ast, str>,
        functions: Vec<FunctionHeader<'ast>>,
    },
    Return(Expression),
    Conditional {
        condition: Box<Expression>,
        truthy: Box<Statement<'ast, Expression>>,
        falsy: Option<Box<Statement<'ast, Expression>>>,
    },
    TypeAlias {
        name: std::borrow::Cow<'ast, str>,
        explicit_type: Type<'ast>,
    },
}

impl<Expression: Display> Display for StatementValue<'_, Expression> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
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

impl<Expression: Display> Display for Statement<'_, Expression> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl<Expression: Display> Statement<'_, Expression> {
    pub fn label(&self, label: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, label)
    }
}

#[derive(Debug, Clone)]
pub struct FunctionHeader<'ast> {
    pub name: Cow<'ast, str>,
    pub parameters: Vec<ParameterDeclaration<'ast>>,
    pub explicit_return_type: Option<Type<'ast>>,
    pub span: miette::SourceSpan,
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
