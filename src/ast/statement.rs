use super::{Expression, TypedExpression, Typing};

use span_derive::Span;
use std::borrow::Cow;

pub type TypedStatement<'ast> = GenericStatement<'ast, TypedExpression<'ast>>;
pub type Statement<'ast> = GenericStatement<'ast, Expression<'ast>>;

#[derive(Debug, Clone, Span)]
pub struct GenericStatement<'ast, Expression> {
    pub value: StatementValue<'ast, Expression>,
    pub span: miette::SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StatementValue<'ast, Expression> {
    Block(Vec<GenericStatement<'ast, Expression>>),
    Expression(Expression),
    Declaration(Cow<'ast, str>, Expression),
    Condition(Expression, Box<GenericStatement<'ast, Expression>>),
    WhileLoop(Expression, Box<GenericStatement<'ast, Expression>>),
    Function(GenericFunctionDeclaration<'ast, Expression>),
    Intrinsic(IntrinsicFunctionDeclaration<'ast>),
}

impl<'ast> StatementValue<'ast, Expression<'ast>> {
    pub fn with_span(self, span: miette::SourceSpan) -> Statement<'ast> {
        Statement { value: self, span }
    }
}

impl<'ast> GenericStatement<'ast, Expression<'ast>> {
    pub fn expression(
        span: miette::SourceSpan,
        value: Expression<'ast>,
    ) -> GenericStatement<'ast, Expression<'ast>> {
        Self {
            value: StatementValue::Expression(value),
            span,
        }
    }
}

#[derive(Debug, Clone, Span)]
pub struct StructMember<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub ty: Typing<'ast>,
}

pub type TypedFunctionDeclaration<'ast> = GenericFunctionDeclaration<'ast, TypedExpression<'ast>>;
pub type FunctionDeclaration<'ast> = GenericFunctionDeclaration<'ast, Expression<'ast>>;

#[derive(Debug, Clone, Span)]
pub struct Parameter<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub ty: Typing<'ast>,
}

#[derive(Debug, Clone, Span)]
pub struct StructDeclaration<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub members: Vec<StructMember<'ast>>,
}

#[derive(Debug, Clone, Span)]
pub struct GenericFunctionDeclaration<'ast, Expression> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter<'ast>>,
    pub body: Expression,
    pub explicit_return_type: Option<Typing<'ast>>,
}

#[derive(Debug, Clone, Span)]
pub struct IntrinsicFunctionDeclaration<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter<'ast>>,
    pub return_type: Typing<'ast>,
}
