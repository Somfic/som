use miette::SourceSpan;

use super::Expression;
use super::GenericFunctionDeclaration;
use super::IntrinsicFunctionDeclaration;
use super::TypedExpression;

pub type TypedModule<'ast> = GenericModule<'ast, TypedExpression<'ast>>;
pub type Module<'ast> = GenericModule<'ast, Expression<'ast>>;

#[derive(Debug, Clone)]
pub struct GenericModule<'ast, Expression> {
    pub intrinsic_functions: Vec<IntrinsicFunctionDeclaration<'ast>>,
    pub functions: Vec<GenericFunctionDeclaration<'ast, Expression>>,
}
