use crate::prelude::*;
use std::collections::HashMap;

use crate::lexer::Identifier;

pub struct Environment<'env> {
    pub parent: Option<&'env Environment<'env>>,
    pub declarations: HashMap<Identifier, Type>,
    pub type_declarations: HashMap<Identifier, Type>,
}

impl<'env> Environment<'env> {
    pub fn new() -> Self {
        Self {
            parent: None,
            declarations: HashMap::new(),
            type_declarations: HashMap::new(),
        }
    }

    pub fn block(&'env self) -> Self {
        Environment {
            parent: Some(self),
            declarations: HashMap::new(),
            type_declarations: HashMap::new(),
        }
    }

    pub fn function(&'env self) -> Self {
        Environment {
            parent: Some(self),
            declarations: HashMap::new(),
            type_declarations: HashMap::new(),
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

    pub fn get_type(&self, identifier: &Identifier) -> Option<Type> {
        if let Some(type_) = self.type_declarations.get(identifier) {
            return Some(type_.clone());
        }

        if let Some(parent) = self.parent {
            return parent.get_type(identifier);
        }

        None
    }

    pub fn declare(&mut self, identifier: &Identifier, type_: &Type) {
        self.declarations.insert(identifier.clone(), type_.clone());
    }

    pub fn declare_type(&mut self, identifier: &Identifier, type_: &Type) {
        self.type_declarations
            .insert(identifier.clone(), type_.clone());
    }

    pub fn get_all(&self) -> HashMap<Identifier, Type> {
        let mut all = self.declarations.clone();
        if let Some(parent) = self.parent {
            all.extend(parent.get_all());
        }
        all
    }
}
