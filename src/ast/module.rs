use super::Expression;
use super::TypedExpression;
use super::Typing;
use std::{borrow::Cow, collections::HashMap};

pub type TypedModule<'ast> = GenericModule<'ast, TypedExpression<'ast>>;
pub type Module<'ast> = GenericModule<'ast, Expression<'ast>>;

#[derive(Debug, Clone)]
pub struct GenericModule<'ast, Expression> {
    pub functions: Vec<FunctionDeclaration<'ast, Expression>>,
}

pub type TypedFunctionDeclaration<'ast> = FunctionDeclaration<'ast, TypedExpression<'ast>>;
pub type FunctionDeclaration<'ast, Expression> = GenericFunctionDeclaration<'ast, Expression>;

#[derive(Debug, Clone)]
pub struct GenericFunctionDeclaration<'ast, Expression> {
    pub name: Cow<'ast, str>,
    pub parameters: HashMap<Cow<'ast, str>, Typing<'ast>>,
    pub expression: Expression,
}
