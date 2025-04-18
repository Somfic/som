use std::{borrow::Cow, collections::HashMap};

use miette::LabeledSpan;

use crate::{
    ast::{Identifier, TypedFunctionDeclaration, Typing},
    ParserResult,
};

#[derive(Debug, Clone)]
pub struct Environment<'env, 'ast> {
    parent: Option<&'env Environment<'env, 'ast>>,
    variables: HashMap<Identifier<'ast>, Typing<'ast>>,
    types: HashMap<Identifier<'ast>, Typing<'ast>>,
    functions: HashMap<Identifier<'ast>, TypedFunctionDeclaration<'ast>>,
}

pub enum EnvironmentType<'ast> {
    Primitive(Typing<'ast>),
}

impl<'env, 'ast> Environment<'env, 'ast> {
    pub fn new() -> Self {
        Self {
            parent: None,
            types: HashMap::new(),
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn block(&'env self) -> Self {
        Self {
            parent: Some(self),
            types: HashMap::new(),
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn declare_variable(&mut self, name: Identifier<'ast>, ty: Typing<'ast>) {
        self.variables.insert(name, ty);
    }

    pub fn declare_type(&mut self, name: Identifier<'ast>, ty: Typing<'ast>) -> ParserResult<()> {
        if let Some(existing_type) = self.lookup_type(name.as_ref()) {
            return Err(vec![miette::diagnostic!(
                labels = vec![
                    LabeledSpan::at(ty.span, "duplicate type name"),
                    LabeledSpan::at(existing_type.span, "type previously declared here")
                ],
                help = format!("choose a different name for this type"),
                "type `{}` already declared",
                name
            )]);
        }

        self.types.insert(name, ty);

        Ok(())
    }

    pub fn assign_variable(&mut self, name: Identifier<'ast>, ty: Typing<'ast>) {
        match self.lookup_variable(name.as_ref()) {
            Some(existing_type) => {
                if existing_type != &ty {
                    panic!("type mismatch");
                } else {
                    self.variables.insert(name, ty);
                }
            }
            None => {
                panic!("variable not found");
            }
        }
    }

    pub fn lookup_variable(&self, name: &str) -> Option<&Typing<'ast>> {
        self.variables
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_variable(name)))
    }

    pub fn lookup_type(&self, name: &str) -> Option<&Typing<'ast>> {
        self.types
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_type(name)))
    }

    pub fn declare_function(
        &mut self,
        name: Identifier<'ast>,
        function: TypedFunctionDeclaration<'ast>,
    ) -> ParserResult<()> {
        if let Some(existing_function) = self.lookup_function(name.as_ref()) {
            return Err(vec![miette::diagnostic!(
                labels = vec![
                    LabeledSpan::at(function.span, "duplicate function name"),
                    LabeledSpan::at(existing_function.span, "function name previously used here")
                ],
                help = format!("choose a different name for this function"),
                "function `{}` already declared",
                existing_function.name
            )]);
        }

        self.functions.insert(name, function);

        Ok(())
    }

    pub fn update_function(
        &mut self,
        name: Identifier<'ast>,
        function: TypedFunctionDeclaration<'ast>,
    ) -> ParserResult<()> {
        self.functions.insert(name, function);

        Ok(())
    }

    pub fn lookup_function(&self, name: &str) -> Option<&TypedFunctionDeclaration<'ast>> {
        self.functions
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_function(name)))
    }
}
