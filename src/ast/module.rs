
use super::Expression;
use super::GenericFunctionDeclaration;
use super::IntrinsicFunctionDeclaration;
use super::TypedExpression;

pub type TypedModule = GenericModule<TypedExpression>;
pub type Module = GenericModule<Expression>;

#[derive(Debug, Clone)]
pub struct GenericModule<Expression> {
    pub intrinsic_functions: Vec<IntrinsicFunctionDeclaration>,
    pub functions: Vec<GenericFunctionDeclaration<Expression>>,
}
