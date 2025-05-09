use super::{Expression, Identifier, Parameter, TypedExpression, Typing};
use span_derive::Span;
use std::fmt::Display;
pub type TypedStatement = GenericStatement<TypedExpression>;
pub type Statement = GenericStatement<Expression>;

#[derive(Debug, Clone, Span)]
pub struct GenericStatement<Expression> {
    pub value: StatementValue<Expression>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StatementValue<Expression> {
    Block(Vec<GenericStatement<Expression>>),
    Expression(Expression),
    Condition(Expression, Box<GenericStatement<Expression>>),
    WhileLoop(Expression, Box<GenericStatement<Expression>>),
    Declaration(Identifier, Option<Box<Typing>>, Box<Expression>),
    TypeDeclaration(Identifier, Box<Typing>),
}

impl StatementValue<Expression> {
    pub fn with_span(self, span: miette::SourceSpan) -> Statement {
        Statement { value: self, span }
    }
}

impl GenericStatement<Expression> {
    pub fn expression(span: miette::SourceSpan, value: Expression) -> GenericStatement<Expression> {
        Self {
            value: StatementValue::Expression(value),
            span,
        }
    }
}

pub type TypedFunction = GenericFunction<TypedExpression>;
pub type Function = GenericFunction<Expression>;

#[derive(Debug, Clone)]
pub struct GenericFunction<Expression> {
    pub identifier: Identifier,
    pub signature: FunctionSignature,
    pub body: Box<Expression>,
}

#[derive(Debug, Clone, Span, Eq)]
pub struct FunctionSignature {
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter>,
    pub explicit_return_type: Option<Box<Typing>>,
}

impl PartialEq for FunctionSignature {
    fn eq(&self, other: &Self) -> bool {
        self.parameters == other.parameters
            && self.explicit_return_type == other.explicit_return_type
    }
}

#[derive(Debug, Clone, Eq)]
pub struct IntrinsicSignature {
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter>,
    pub return_type: Box<Typing>,
}

impl PartialEq for IntrinsicSignature {
    fn eq(&self, other: &Self) -> bool {
        self.parameters == other.parameters && self.return_type == other.return_type
    }
}

#[derive(Debug, Clone, Eq)]
pub struct LambdaSignature {
    pub parameters: Vec<LambdaSignatureParameter>,
    pub return_type: Box<Typing>,
}

#[derive(Debug, Clone, Eq, Span)]
pub struct LambdaSignatureParameter {
    pub name: Option<Box<str>>,
    pub ty: Typing,
    pub span: miette::SourceSpan,
}

impl PartialEq for LambdaSignature {
    fn eq(&self, other: &Self) -> bool {
        self.parameters == other.parameters && self.return_type == other.return_type
    }
}

impl PartialEq for LambdaSignatureParameter {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.ty == other.ty
    }
}

impl Display for LambdaSignatureParameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = &self.name {
            write!(f, "{} ~ {}", name, self.ty)
        } else {
            write!(f, "{}", self.ty)
        }
    }
}

impl Display for LambdaSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "({}) -> {}",
            self.parameters
                .iter()
                .map(|param| param.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            self.return_type
        )
    }
}

impl LambdaSignatureParameter {
    pub fn to_parameter(&self) -> Parameter {
        Parameter {
            identifier: Identifier {
                name: self.name.clone().unwrap_or_default(),
                span: self.span,
            },
            ty: self.ty.clone(),
            span: self.span,
        }
    }
}
