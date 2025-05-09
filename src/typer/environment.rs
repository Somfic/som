use crate::{
    ast::{Identifier, TypedFunction, Typing},
    Result,
};
use miette::LabeledSpan;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Environment<'parent> {
    parent: Option<&'parent Environment<'parent>>,
    declarations: HashMap<Box<str>, Typing>,
    types: HashMap<Box<str>, Typing>,
}

impl<'parent> Environment<'parent> {
    pub fn new() -> Self {
        Self {
            parent: None,
            types: HashMap::new(),
            declarations: HashMap::new(),
        }
    }

    pub fn block(&'parent self) -> Self {
        Self {
            parent: Some(self),
            types: HashMap::new(),
            declarations: HashMap::new(),
        }
    }

    pub fn declare(&mut self, identifier: &Identifier, ty: &Typing) {
        self.declarations
            .insert(identifier.name.clone(), ty.clone());
    }

    pub fn declare_type(&mut self, identifier: &Identifier, ty: &Typing) -> Result<()> {
        if let Some(existing_type) = self.lookup_type(identifier) {
            return Err(vec![miette::diagnostic!(
                labels = vec![
                    LabeledSpan::at(ty.span, "duplicate type name"),
                    LabeledSpan::at(existing_type.span, "type previously declared here")
                ],
                help = format!("choose a different name for this type"),
                "type `{}` already declared",
                identifier
            )]);
        }

        self.types.insert(identifier.name.clone(), ty.clone());

        Ok(())
    }

    pub fn assign(&mut self, identifier: &Identifier, ty: &Typing) {
        match self.lookup(identifier) {
            Some(existing_type) => {
                if *existing_type != *ty {
                    panic!("type mismatch");
                } else {
                    self.declarations
                        .insert(identifier.name.clone(), ty.clone());
                }
            }
            None => {
                panic!("variable not found");
            }
        }
    }

    pub fn lookup(&self, identifier: &Identifier) -> Option<&Typing> {
        self.declarations
            .get(&identifier.name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup(identifier)))
    }

    pub fn lookup_type(&self, identifier: &Identifier) -> Option<&Typing> {
        self.types
            .get(&identifier.name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_type(identifier)))
    }
}
