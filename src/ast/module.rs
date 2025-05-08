use super::Expression;
use super::GenericStatement;
use super::TypedExpression;

pub type TypedModule = GenericModule<TypedExpression>;
pub type Module = GenericModule<Expression>;

#[derive(Debug, Clone)]
pub struct GenericModule<Expression> {
    pub statements: Vec<GenericStatement<Expression>>,
}
