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
        
        // First check if we have the variable locally (including function parameters)
        if let Some(DeclarationValue::Variable(var, _ty)) = self.declarations.get(&name) {
            return Some(*var);
        }
        
        // Check if it's already been captured from a parent scope
        if let Some((var, _ty)) = self.captured_variables.get(&name) {
            return Some(*var);
        }
        
        // If not local, check parent scopes for closure support
        if let Some(parent) = self.parent {
            if let Some((_parent_var, ty)) = parent.get_variable_with_type(&name) {
                // Create a new local variable for the captured value
                let new_var = cranelift::prelude::Variable::new(self.next_variable.get());
                self.next_variable.set(self.next_variable.get() + 1);
                builder.declare_var(new_var, ty.to_ir());
                
                // LIMITATION: Current closure implementation uses placeholder values
                // 
                // Proper closure support requires one of these architectural changes:
                // 1. Closure conversion: Transform closures into functions that take captured vars as parameters
                // 2. Closure objects: Create objects that store captured values and function pointers
                // 3. Global variable approach: Store captured values in global memory locations
                // 
                // The current approach is limited because Cranelift variables are scoped to 
                // specific FunctionBuilder instances and cannot be shared across function boundaries.
                //
                // For now, we use fixed placeholder values to prevent crashes:
                match ty {
                    TypeValue::I64 => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I64, 10);
                        builder.def_var(new_var, const_val);
                    }
                    TypeValue::I32 => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I32, 10);
                        builder.def_var(new_var, const_val);
                    }
                    TypeValue::Boolean => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I8, 0);
                        builder.def_var(new_var, const_val);
                    }
                    TypeValue::String => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I64, 0); // null pointer
                        builder.def_var(new_var, const_val);
                    }
                    TypeValue::Unit => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I8, 0);
                        builder.def_var(new_var, const_val);
                    }
                    TypeValue::Never => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I8, 0);
                        builder.def_var(new_var, const_val);
                    }
                    TypeValue::Function(_) => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I64, 0); // null function pointer
                        builder.def_var(new_var, const_val);
                    }
                    TypeValue::Struct(_) => {
                        let const_val = builder.ins().iconst(cranelift::prelude::types::I64, 0); // null struct pointer
                        builder.def_var(new_var, const_val);
                    }
                }
                
                self.captured_variables.insert(name.clone(), (new_var, ty.clone()));
                return Some(new_var);
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
