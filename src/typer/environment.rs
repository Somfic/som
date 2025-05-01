use crate::{
    ast::{Identifier, TypedFunctionDeclaration, Typing},
    ParserResult,
};
use miette::LabeledSpan;
use std::{borrow::Cow, collections::HashMap};

#[derive(Debug, Clone)]
pub struct Environment<'ast> {
    parent: Option<Box<Environment<'ast>>>,
    variables: HashMap<Cow<'ast, str>, Typing<'ast>>,
    types: HashMap<Cow<'ast, str>, Typing<'ast>>,
    functions: HashMap<Cow<'ast, str>, TypedFunctionDeclaration<'ast>>,
}

impl<'ast> Environment<'ast> {
    pub fn new() -> Self {
        Self {
            parent: None,
            types: HashMap::new(),
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn block(&self) -> Self {
        Self {
            parent: Some(Box::new(self.clone())),
            types: HashMap::new(),
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn declare_variable(&mut self, identifier: &Identifier<'ast>, ty: &Typing<'ast>) {
        self.variables.insert(identifier.name.clone(), ty.clone());
    }

    pub fn declare_type(
        &mut self,
        identifier: &Identifier<'ast>,
        ty: &Typing<'ast>,
    ) -> ParserResult<()> {
        if let Some(existing_type) = self.lookup_type(&identifier) {
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

    pub fn assign_variable(&mut self, identifier: &Identifier<'ast>, ty: &Typing<'ast>) {
        match self.lookup_variable(identifier) {
            Some(existing_type) => {
                if existing_type != ty {
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

    pub fn lookup_variable(
        &'ast self,
        identifier: &Identifier<'ast>,
    ) -> Option<&'ast Typing<'ast>> {
        self.variables
            .get(&identifier.name)
            .or_else(|| { 
                self.parent
                    .as_ref()
                    .and_then(|p| p.lookup_variable(identifier))
            })
            .map(|t| t.unzip(self))
    }

    pub fn lookup_type(&'ast self, identifier: &Identifier<'ast>) -> Option<&'ast Typing<'ast>> {
        self.types
            .get(&identifier.name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_type(identifier)))
            .map(|t| t.unzip(self))
    }

    pub fn declare_function(
        &mut self,
        identifier: &Identifier<'ast>,
        function: &TypedFunctionDeclaration<'ast>,
    ) -> ParserResult<()> {
        if let Some(existing_function) = self.lookup_function(&identifier) {
            return Err(vec![miette::diagnostic!(
                labels = vec![
                    LabeledSpan::at(function.span, "duplicate function name"),
                    LabeledSpan::at(existing_function.span, "function name previously used here")
                ],
                help = format!("choose a different name for this function"),
                "function `{}` already declared",
                existing_function.identifier
            )]);
        }

        self.update_function(identifier, function)?;

        Ok(())
    }

    pub fn update_function(
        &mut self,
        identifier: &Identifier<'ast>,
        function: &TypedFunctionDeclaration<'ast>,
    ) -> ParserResult<()> {
        self.functions
            .insert(identifier.name.clone(), function.clone());

        Ok(())
    }

    pub fn lookup_function(
        &'ast self,
        identifier: &Identifier<'ast>,
    ) -> Option<&'ast TypedFunctionDeclaration<'ast>> {
        self.functions.get(&identifier.name).or_else(|| {
            self.parent
                .as_ref()
                .and_then(|p| p.lookup_function(identifier))
        })
    }
}
