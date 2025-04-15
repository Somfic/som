use std::borrow::Cow;

use miette::SourceSpan;

use super::{Expression, TypedExpression, Typing};

pub type TypedStatement<'ast> = GenericStatement<'ast, TypedExpression<'ast>>;
pub type Statement<'ast> = GenericStatement<'ast, Expression<'ast>>;

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct StructMember<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub ty: Typing<'ast>,
}

impl StructMember<'_> {
    pub fn label(&self, text: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, text.into())
    }

    pub fn span(mut self, span: SourceSpan) -> Self {
        self.span = span;
        self
    }
}

pub type TypedFunctionDeclaration<'ast> = GenericFunctionDeclaration<'ast, TypedExpression<'ast>>;
pub type FunctionDeclaration<'ast> = GenericFunctionDeclaration<'ast, Expression<'ast>>;

#[derive(Debug, Clone)]
pub struct Parameter<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub ty: Typing<'ast>,
}

impl Parameter<'_> {
    pub fn label(&self, text: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, text.into())
    }

    pub fn span(mut self, span: SourceSpan) -> Self {
        self.span = span;
        self
    }
}

#[derive(Debug, Clone)]
pub struct StructDeclaration<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub members: Vec<StructMember<'ast>>,
}

#[derive(Debug, Clone)]
pub struct GenericFunctionDeclaration<'ast, Expression> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter<'ast>>,
    pub body: Expression,
    pub explicit_return_type: Option<Typing<'ast>>,
}

#[derive(Debug, Clone)]
pub struct IntrinsicFunctionDeclaration<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub parameters: Vec<Parameter<'ast>>,
    pub return_type: Typing<'ast>,
}

impl TypedFunctionDeclaration<'_> {
    pub fn label(&self, text: impl Into<String>) -> miette::LabeledSpan {
        miette::LabeledSpan::at(self.span, text.into())
    }

    pub fn span(mut self, span: SourceSpan) -> Self {
        self.span = span;
        self
    }
}
