use std::{borrow::Cow, collections::HashMap};

use crate::ast::{Type, TypeValue};

#[derive(Debug, Clone)]
pub struct Environment<'env, 'ast> {
    parent: Option<&'env Environment<'env, 'ast>>,
    bindings: HashMap<Cow<'ast, str>, Type<'ast>>,
}

pub enum EnvironmentType<'ast> {
    Primitive(Type<'ast>),
}

impl<'env, 'ast> Environment<'env, 'ast> {
    pub fn new() -> Self {
        Self {
            parent: None,
            bindings: HashMap::new(),
        }
    }

    pub fn block(&'env self) -> Self {
        Self {
            parent: Some(self),
            bindings: HashMap::new(),
        }
    }

    pub fn declare(&mut self, name: Cow<'ast, str>, ty: Type<'ast>) {
        if let TypeValue::Symbol(symbol_name) = ty.base_type().value.clone() {
            match self.lookup(&symbol_name) {
                Some(symbol) => {
                    self.bindings.insert(name, symbol.clone());
                }
                None => {
                    self.bindings.insert(name, ty);
                }
            }
        } else {
            self.bindings.insert(name, ty);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Type<'ast>> {
        self.bindings
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup(name)))
            .map(|ty| ty.base_type())
    }
}
