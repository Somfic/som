use std::{borrow::Cow, collections::HashMap};

use miette::LabeledSpan;

use crate::{
    ast::{TypedFunctionDeclaration, Typing, TypingValue},
    ParserResult,
};

#[derive(Debug, Clone)]
pub struct Environment<'env, 'ast> {
    parent: Option<&'env Environment<'env, 'ast>>,
    variables: HashMap<Cow<'ast, str>, Typing<'ast>>,
    functions: HashMap<Cow<'ast, str>, TypedFunctionDeclaration<'ast>>,
}

pub enum EnvironmentType<'ast> {
    Primitive(Typing<'ast>),
}

impl<'env, 'ast> Environment<'env, 'ast> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn block(&'env self) -> Self {
        Self {
            parent: Some(self),
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    pub fn declare_variable(&mut self, name: Cow<'ast, str>, ty: Typing<'ast>) {
        self.variables.insert(name, ty);
    }

    pub fn lookup_variable(&self, name: &str) -> Option<&Typing<'ast>> {
        self.variables
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_variable(name)))
    }

    pub fn declare_function(
        &mut self,
        name: Cow<'ast, str>,
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

    pub fn lookup_function(&self, name: &str) -> Option<&TypedFunctionDeclaration<'ast>> {
        self.functions
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_function(name)))
    }
}
