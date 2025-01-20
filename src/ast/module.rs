use std::borrow::Cow;

use super::Statement;

#[derive(Debug, Clone)]
pub struct Module<'ast, Expression> {
    pub name: Cow<'ast, str>,
    pub definitions: Vec<Statement<'ast, Expression>>,
}
