use miette::SourceSpan;

use super::Expression;
use super::TypedExpression;
use super::Typing;
use std::{borrow::Cow, collections::HashMap};

pub type TypedModule<'ast> = GenericModule<'ast, TypedExpression<'ast>>;
pub type Module<'ast> = GenericModule<'ast, Expression<'ast>>;

#[derive(Debug, Clone)]
pub struct GenericModule<'ast, Expression> {
    pub intrinsic_functions: Vec<IntrinsicFunctionDeclaration<'ast>>,
    pub functions: Vec<GenericFunctionDeclaration<'ast, Expression>>,
}

pub type TypedFunctionDeclaration<'ast> = GenericFunctionDeclaration<'ast, TypedExpression<'ast>>;
pub type FunctionDeclaration<'ast> = GenericFunctionDeclaration<'ast, Expression<'ast>>;

#[derive(Debug, Clone)]
pub struct Paramater<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub ty: Typing<'ast>,
}

impl Paramater<'_> {
    pub fn label(&self, text: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, text.into())
    }

    pub fn span(mut self, span: SourceSpan) -> Self {
        self.span = span;
        self
    }
}

#[derive(Debug, Clone)]
pub struct GenericFunctionDeclaration<'ast, Expression> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Paramater<'ast>>,
    pub body: Expression,
    pub explicit_return_type: Option<Typing<'ast>>,
}

#[derive(Debug, Clone)]
pub struct IntrinsicFunctionDeclaration<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Paramater<'ast>>,
    pub return_type: Typing<'ast>,
}
