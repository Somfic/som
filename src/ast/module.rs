use super::Expression;
use super::Type;
use super::TypedExpression;
use std::{borrow::Cow, collections::HashMap, hash::Hash};

pub type TypedModule<'ast> = GenericModule<'ast, TypedExpression<'ast>>;
pub type Module<'ast> = GenericModule<'ast, Expression<'ast>>;

#[derive(Debug, Clone)]
pub struct GenericModule<'ast, Expression> {
    pub functions: HashMap<Cow<'ast, str>, FunctionDeclaration<'ast, Expression>>,
}

#[derive(Debug, Clone)]
pub struct FunctionDeclaration<'ast, Expression> {
    pub parameters: HashMap<Cow<'ast, str>, Type<'ast>>,
    pub expression: Expression,
}
