use crate::parser::ast::Type;
use std::{borrow::Cow, collections::HashMap};

pub struct Environment<'env, 'ast> {
    parent: Option<&'env Environment<'env, 'ast>>,
    bindings: HashMap<Cow<'env, str>, Type<'ast>>,
}

pub enum EnvironmentType<'ast> {
    Primitive(Type<'ast>),
    Alias(Cow<'ast, str>),
}

impl<'env, 'ast> Environment<'env, 'ast> {
    pub fn new(parent: Option<&'env Environment<'env, 'ast>>) -> Self {
        Self {
            parent,
            bindings: HashMap::new(),
        }
    }

    pub fn set(&mut self, name: Cow<'env, str>, ty: Type<'ast>) {
        self.bindings.insert(name, ty);
    }

    pub fn get(&self, name: &str) -> Option<&Type<'ast>> {
        self.bindings
            .get(name)
            .or_else(|| self.parent.and_then(|p| p.get(name)))
    }
}
