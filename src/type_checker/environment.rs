use crate::prelude::*;
use std::collections::HashMap;

use crate::lexer::Identifier;

pub struct Environment<'env> {
    pub parent: Option<&'env Environment<'env>>,
    pub declarations: HashMap<Identifier, Type>,
}

impl<'env> Environment<'env> {
    pub fn new() -> Self {
        Self {
            parent: None,
            declarations: HashMap::new(),
        }
    }

    pub fn block(&'env self) -> Self {
        Environment {
            parent: Some(self),
            declarations: HashMap::new(),
        }
    }

    pub fn get(&self, identifier: &Identifier) -> Option<Type> {
        if let Some(type_) = self.declarations.get(identifier) {
            return Some(type_.clone());
        }

        if let Some(parent) = self.parent {
            return parent.get(identifier);
        }

        None
    }

    pub fn set(&mut self, identifier: &Identifier, type_: &Type) {
        self.declarations.insert(identifier.clone(), type_.clone());
    }
}
