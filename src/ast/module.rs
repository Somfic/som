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
pub struct GenericFunctionDeclaration<'ast, Expression> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub parameters: HashMap<Cow<'ast, str>, Typing<'ast>>,
    pub body: Expression,
    pub explicit_return_type: Option<Typing<'ast>>,
}

#[derive(Debug, Clone)]
pub struct IntrinsicFunctionDeclaration<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
    pub parameters: HashMap<Cow<'ast, str>, Typing<'ast>>,
    pub return_type: Typing<'ast>,
}
