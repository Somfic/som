use cranelift_module::FuncId;

use crate::prelude::*;
use std::collections::HashMap;

use crate::lexer::Identifier;

pub struct Environment<'env> {
    pub parent: Option<&'env Environment<'env>>,
    pub declarations: HashMap<Identifier, DeclarationValue>,
}

#[derive(Debug, Clone)]
enum DeclarationValue {
    Function(FuncId),
    Variable(Type),
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

    pub fn get(&self, identifier: &Identifier) -> Option<DeclarationValue> {
        if let Some(declaration) = self.declarations.get(identifier) {
            return Some(declaration.clone());
        }

        if let Some(parent) = self.parent {
            return parent.get(identifier);
        }

        None
    }

    pub fn get_function(&self, identifier: &Identifier) -> Option<FuncId> {
        match self.get(identifier) {
            Some(DeclarationValue::Function(func_id)) => Some(func_id),
            _ => None,
        }
    }

    pub fn set_function(&mut self, identifier: &Identifier, func_id: &FuncId, signature:) {
        self.declarations.insert(
            identifier.clone(),
            DeclarationValue::Function(func_id.clone()),
        );
    }
}
