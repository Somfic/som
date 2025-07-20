use cranelift::prelude::EntityRef;
use cranelift_module::FuncId;

use crate::prelude::*;
use std::{cell::Cell, collections::HashMap, rc::Rc};

use crate::lexer::Identifier;

pub struct Environment<'env> {
    pub parent: Option<&'env Environment<'env>>,
    pub declarations: HashMap<String, DeclarationValue>,
    next_variable: Rc<Cell<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationValue {
    Variable(cranelift::prelude::Variable, TypeValue),
    Function(FuncId),
}

impl<'env> Environment<'env> {
    pub fn new(declarations: HashMap<String, DeclarationValue>) -> Self {
        Self {
            parent: None,
            next_variable: Rc::new(Cell::new(0)),
            declarations,
        }
    }

    pub fn block(&'env self) -> Self {
        Environment {
            parent: Some(self),
            next_variable: self.next_variable.clone(),
            declarations: HashMap::new(),
        }
    }

    pub fn declare_function(&mut self, identifier: impl Into<String>, func_id: FuncId) -> FuncId {
        self.declarations
            .insert(identifier.into(), DeclarationValue::Function(func_id));

        func_id
    }

    pub fn declare_variable(
        &mut self,
        identifier: impl Into<String>,
        builder: &mut FunctionBuilder,
        ty: &TypeValue,
    ) -> cranelift::prelude::Variable {
        let var = cranelift::prelude::Variable::new(self.next_variable.get());
        self.next_variable.set(self.next_variable.get() + 1);
        self.declarations
            .insert(identifier.into(), DeclarationValue::Variable(var, ty.clone()));
        builder.declare_var(var, ty.to_ir());
        var
    }

    pub fn get_variable(
        &self,
        identifier: impl Into<String>,
    ) -> Option<cranelift::prelude::Variable> {
        let name = identifier.into();
        if let Some(DeclarationValue::Variable(var, _ty)) = self.declarations.get(&name) {
            Some(*var)
        } else if let Some(parent) = self.parent {
            // recurse into parent scope
            parent.get_variable(name)
        } else {
            None
        }
    }

    /// Get a variable and ensure it's declared in the current function builder scope.
    /// This is needed when accessing variables from parent scopes in nested functions.
    pub fn get_variable_with_declaration(
        &mut self,
        identifier: impl Into<String>,
        builder: &mut FunctionBuilder,
    ) -> Option<cranelift::prelude::Variable> {
        let name = identifier.into();
        
        // First check if we have the variable locally
        if let Some(DeclarationValue::Variable(var, _ty)) = self.declarations.get(&name) {
            return Some(*var);
        }
        
        // If not local, check parent scopes
        if let Some(parent) = self.parent {
            if let Some((var, ty)) = parent.get_variable_with_type(&name) {
                // Declare the variable in the current builder so Cranelift knows about it
                // Only declare if we haven't already declared it in this scope
                if !self.declarations.contains_key(&name) {
                    builder.declare_var(var, ty.to_ir());
                    // Add it to our local declarations to avoid re-declaring
                    self.declarations.insert(name.clone(), DeclarationValue::Variable(var, ty.clone()));
                }
                return Some(var);
            }
        }
        
        None
    }

    /// Get a variable and its type from this environment or parent scopes
    fn get_variable_with_type(&self, identifier: &str) -> Option<(cranelift::prelude::Variable, &TypeValue)> {
        if let Some(DeclarationValue::Variable(var, ty)) = self.declarations.get(identifier) {
            Some((*var, ty))
        } else if let Some(parent) = self.parent {
            parent.get_variable_with_type(identifier)
        } else {
            None
        }
    }

    /// lookup a function, searching parents if needed
    pub fn get_function(&self, identifier: impl Into<String>) -> Option<FuncId> {
        let name = identifier.into();
        if let Some(DeclarationValue::Function(func_id)) = self.declarations.get(&name) {
            Some(*func_id)
        } else if let Some(parent) = self.parent {
            // recurse into parent scope
            parent.get_function(name)
        } else {
            None
        }
    }
}
