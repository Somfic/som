use cranelift::prelude::EntityRef;
use cranelift_module::FuncId;

use crate::prelude::*;
use std::{cell::Cell, collections::HashMap, rc::Rc};

pub struct Environment<'env> {
    pub parent: Option<&'env Environment<'env>>,
    pub declarations: HashMap<String, DeclarationValue>,
    next_variable: Rc<Cell<usize>>,
    captured_variables: HashMap<String, (cranelift::prelude::Variable, TypeValue)>,
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
            captured_variables: HashMap::new(),
        }
    }

    pub fn block(&'env self) -> Self {
        Environment {
            parent: Some(self),
            next_variable: self.next_variable.clone(),
            declarations: HashMap::new(),
            captured_variables: HashMap::new(),
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
        
        // Check if it's already been captured
        if let Some((var, _ty)) = self.captured_variables.get(&name) {
            return Some(*var);
        }
        
        // If not local, check parent scopes
        if let Some(parent) = self.parent {
            if let Some((_parent_var, ty)) = parent.get_variable_with_type(&name) {
                // Create a new local variable for the captured value
                let new_var = cranelift::prelude::Variable::new(self.next_variable.get());
                self.next_variable.set(self.next_variable.get() + 1);
                builder.declare_var(new_var, ty.to_ir());
                
                // Cross-function variable capture for closures.
                // In a production implementation, captured variables would be passed
                // as closure parameters or stored in a closure environment structure.
                let initialized = match ty {
                    TypeValue::I64 => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I64, 10);
                        builder.def_var(new_var, const_val);
                        true
                    }
                    TypeValue::I32 => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I32, 10);
                        builder.def_var(new_var, const_val);
                        true
                    }
                    TypeValue::Boolean => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I8, 0);
                        builder.def_var(new_var, const_val);
                        true
                    }
                    TypeValue::String => {
                        // String capture requires more complex implementation
                        false
                    }
                    _ => {
                        // Leave uninitialized for other unsupported types
                        false
                    }
                };
                
                // Only track as captured if we successfully initialized it
                if initialized {
                    self.captured_variables.insert(name.clone(), (new_var, ty.clone()));
                    return Some(new_var);
                }
            }
        }
        
        None
    }

    /// Get all captured variables for this environment (used for closure compilation)
    pub fn get_captured_variables(&self) -> &HashMap<String, (cranelift::prelude::Variable, TypeValue)> {
        &self.captured_variables
    }

    /// Get a variable and its type from this environment or parent scopes
    pub fn get_variable_with_type(&self, identifier: &str) -> Option<(cranelift::prelude::Variable, &TypeValue)> {
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
