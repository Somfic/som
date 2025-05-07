use super::{Expression, Identifier, TypedExpression, Typing};

use span_derive::Span;
use std::collections::HashMap;

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
    VariableDeclaration(Identifier, Option<Typing>, Expression),
    FunctionDeclaration(GenericFunctionDeclaration<Expression>),
    IntrinsicDeclaration(IntrinsicFunctionDeclaration),
    TypeDeclaration(Identifier, Typing),
    StructDeclaration {
        identifier: Identifier,
        explicit_type: Option<Typing>,
        struct_type: Typing,
        parameters: HashMap<Identifier, Expression>,
    },
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

pub type TypedFunctionDeclaration = GenericFunctionDeclaration<TypedExpression>;
pub type FunctionDeclaration = GenericFunctionDeclaration<Expression>;

#[derive(Debug, Clone, Span)]
pub struct Parameter {
    pub identifier: Identifier,
    pub span: miette::SourceSpan,
    pub ty: Typing,
}

#[derive(Debug, Clone, Span)]
pub struct GenericFunctionDeclaration<Expression> {
    pub identifier: Identifier,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter>,
    pub body: Expression,
    pub explicit_return_type: Option<Typing>,
}

#[derive(Debug, Clone, Span)]
pub struct IntrinsicFunctionDeclaration {
    pub identifier: Identifier,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter>,
    pub return_type: Typing,
}
