use crate::{
    ast::{Function, Identifier, TypedFunction, Typing},
    Result,
};
use miette::LabeledSpan;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Environment<'parent> {
    parent: Option<&'parent Environment<'parent>>,
    variables: HashMap<Box<str>, Typing>,
    types: HashMap<Box<str>, Typing>,
    functions: HashMap<Box<str>, TypedFunction>,
}

impl<'parent> Environment<'parent> {
    pub fn new() -> Self {
        Self {
            parent: None,
            types: HashMap::new(),
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn block(&'parent self) -> Self {
        Self {
            parent: Some(self),
            types: HashMap::new(),
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn declare_variable(&mut self, identifier: &Identifier, ty: &Typing) {
        self.variables.insert(identifier.name.clone(), ty.clone());
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

    pub fn assign_variable(&mut self, identifier: &Identifier, ty: &Typing) {
        match self.lookup_variable(identifier) {
            Some(existing_type) => {
                if *existing_type != *ty {
                    panic!("type mismatch");
                } else {
                    self.variables.insert(identifier.name.clone(), ty.clone());
                }
            }
            None => {
                panic!("variable not found");
            }
        }
    }

    pub fn lookup_variable(&self, identifier: &Identifier) -> Option<&Typing> {
        self.variables.get(&identifier.name).or_else(|| {
            self.parent
                .as_ref()
                .and_then(|p| p.lookup_variable(identifier))
        })
    }

    pub fn lookup_type(&self, identifier: &Identifier) -> Option<&Typing> {
        self.types
            .get(&identifier.name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_type(identifier)))
    }

    pub fn declare_function(
        &mut self,
        identifier: &Identifier,
        function: &TypedFunction,
    ) -> Result<()> {
        // TODO: check for duplicate function names when they're publicly declared in an impl block, redefining a function inline should be fine
        // if let Some(existing_function) = self.lookup_function(identifier) {
        //     return Err(vec![miette::diagnostic!(
        //         labels = vec![
        //             LabeledSpan::at(function.signature.span, "duplicate function name"),
        //             LabeledSpan::at(
        //                 existing_function.signature.span,
        //                 "function name previously used here"
        //             )
        //         ],
        //         help = format!("choose a different name for this function"),
        //         "function `{}` already declared",
        //         identifier
        //     )]);
        // }

        self.update_function(identifier, function)?;

        Ok(())
    }

    pub fn update_function(
        &mut self,
        identifier: &Identifier,
        function: &TypedFunction,
    ) -> Result<()> {
        self.functions
            .insert(identifier.name.clone(), function.clone());

        Ok(())
    }

    pub fn lookup_function(&self, identifier: &Identifier) -> Option<TypedFunction> {
        self.functions.get(&identifier.name).cloned().or_else(|| {
            self.parent
                .as_ref()
                .and_then(|p| p.lookup_function(identifier))
        })
    }
}
