use std::borrow::Cow;

use miette::SourceSpan;

use super::{
    Expression, GenericFunctionDeclaration, IntrinsicFunctionDeclaration, StructDeclaration,
    TypedExpression, Typing,
};

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
    Struct(StructDeclaration<'ast>),
    Enum(EnumDeclaration<'ast>)
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
