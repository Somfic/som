use std::{borrow::Cow, collections::HashMap};

use crate::ast::{Typing, TypingValue};

#[derive(Debug, Clone)]
pub struct Environment<'env, 'ast> {
    parent: Option<&'env Environment<'env, 'ast>>,
    variables: HashMap<Cow<'ast, str>, Typing<'ast>>,
}

pub enum EnvironmentType<'ast> {
    Primitive(Typing<'ast>),
}

impl<'env, 'ast> Environment<'env, 'ast> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
        }
    }

    pub fn block(&'env self) -> Self {
        Self {
            parent: Some(self),
            variables: HashMap::new(),
        }
    }

    pub fn declare(&mut self, name: Cow<'ast, str>, ty: Typing<'ast>) {
        self.variables.insert(name, ty);
    }

    pub fn lookup(&self, name: &str) -> Option<&Typing<'ast>> {
        self.variables
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup(name)))
    }
}
