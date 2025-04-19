use super::{Expression, Identifier, TypedExpression, Typing};

use span_derive::Span;
use std::{borrow::Cow, collections::HashMap};

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
    Condition(Expression, Box<GenericStatement<'ast, Expression>>),
    WhileLoop(Expression, Box<GenericStatement<'ast, Expression>>),
    VariableDeclaration(Identifier<'ast>, Option<Typing<'ast>>, Expression),
    FunctionDeclaration(GenericFunctionDeclaration<'ast, Expression>),
    IntrinsicDeclaration(IntrinsicFunctionDeclaration<'ast>),
    TypeDeclaration(Identifier<'ast>, Typing<'ast>),
    StructDeclaration(
        Identifier<'ast>,
        Typing<'ast>,
        Option<Typing<'ast>>,
        HashMap<Identifier<'ast>, Expression>,
    ),
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

pub type TypedFunctionDeclaration<'ast> = GenericFunctionDeclaration<'ast, TypedExpression<'ast>>;
pub type FunctionDeclaration<'ast> = GenericFunctionDeclaration<'ast, Expression<'ast>>;

#[derive(Debug, Clone, Span)]
pub struct Parameter<'ast> {
    pub identifier: Identifier<'ast>,
    pub span: miette::SourceSpan,
    pub ty: Typing<'ast>,
}

#[derive(Debug, Clone, Span)]
pub struct GenericFunctionDeclaration<'ast, Expression> {
    pub identifier: Identifier<'ast>,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter<'ast>>,
    pub body: Expression,
    pub explicit_return_type: Option<Typing<'ast>>,
}

#[derive(Debug, Clone, Span)]
pub struct IntrinsicFunctionDeclaration<'ast> {
    pub identifier: Identifier<'ast>,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter<'ast>>,
    pub return_type: Typing<'ast>,
}
