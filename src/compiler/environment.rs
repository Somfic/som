use cranelift::prelude::EntityRef;
use cranelift_module::FuncId;

use crate::prelude::*;
use std::{cell::Cell, collections::HashMap, rc::Rc};

pub struct Environment<'env> {
    pub parent: Option<&'env Environment<'env>>,
    pub declarations: HashMap<String, DeclarationValue>,
    /// Parameter variables for tail call optimization
    pub tail_call_params: Option<Vec<cranelift::prelude::Variable>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationValue {
    Variable(cranelift::prelude::Variable, TypeValue),
    Function(FuncId),
    /// A closure: function ID + captured values
    /// The Vec stores the cranelift Variables that hold the captured values
    Closure(
        FuncId,
        Vec<(String, cranelift::prelude::Variable, TypeValue)>,
    ),
}

impl<'env> Environment<'env> {
    pub fn new(declarations: HashMap<String, DeclarationValue>) -> Self {
        Self {
            parent: None,
            declarations,
            tail_call_params: None,
        }
    }

    pub fn block(&'env self) -> Self {
        Environment {
            parent: Some(self),
            declarations: HashMap::new(),
            tail_call_params: None,
        }
    }

    /// Store parameter variables for tail call optimization
    pub fn set_tail_call_params(&mut self, params: Vec<cranelift::prelude::Variable>) {
        self.tail_call_params = Some(params);
    }

    /// Get parameter variables for tail call optimization
    pub fn get_tail_call_params(&self) -> Option<&Vec<cranelift::prelude::Variable>> {
        if let Some(ref params) = self.tail_call_params {
            Some(params)
        } else if let Some(parent) = self.parent {
            parent.get_tail_call_params()
        } else {
            None
        }
    }

    pub fn declare_function(&mut self, identifier: impl Into<String>, func_id: FuncId) -> FuncId {
        self.declarations
            .insert(identifier.into(), DeclarationValue::Function(func_id));

        func_id
    }

    pub fn declare_closure(
        &mut self,
        identifier: impl Into<String>,
        func_id: FuncId,
        captured_vars: Vec<(String, cranelift::prelude::Variable, TypeValue)>,
    ) -> FuncId {
        self.declarations.insert(
            identifier.into(),
            DeclarationValue::Closure(func_id, captured_vars),
        );

        func_id
    }

    pub fn declare_variable(
        &mut self,
        identifier: impl Into<String>,
        builder: &mut FunctionBuilder,
        ty: &TypeValue,
    ) -> cranelift::prelude::Variable {
        let var = builder.declare_var(ty.to_ir());
        self.declarations.insert(
            identifier.into(),
            DeclarationValue::Variable(var, ty.clone()),
        );
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

    /// Get a variable and its type from this environment or parent scopes
    pub fn get_variable_with_type(
        &self,
        identifier: &str,
    ) -> Option<(cranelift::prelude::Variable, &TypeValue)> {
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
        match self.declarations.get(&name) {
            Some(DeclarationValue::Function(func_id)) => Some(*func_id),
            Some(DeclarationValue::Closure(func_id, _)) => Some(*func_id),
            _ => {
                if let Some(parent) = self.parent {
                    // recurse into parent scope
                    parent.get_function(name)
                } else {
                    None
                }
            }
        }
    }

    /// Get closure information (function ID + captured variables)
    pub fn get_closure(
        &self,
        identifier: impl Into<String>,
    ) -> Option<(
        FuncId,
        Vec<(String, cranelift::prelude::Variable, TypeValue)>,
    )> {
        let name = identifier.into();
        match self.declarations.get(&name) {
            Some(DeclarationValue::Closure(func_id, captured)) => {
                Some((*func_id, captured.clone()))
            }
            _ => {
                if let Some(parent) = self.parent {
                    parent.get_closure(name)
                } else {
                    None
                }
            }
        }
    }

    /// Check if an identifier refers to a closure (vs a plain function)
    pub fn is_closure(&self, identifier: impl Into<String>) -> bool {
        let name = identifier.into();
        match self.declarations.get(&name) {
            Some(DeclarationValue::Closure(_, _)) => true,
            _ => {
                if let Some(parent) = self.parent {
                    parent.is_closure(name)
                } else {
                    false
                }
            }
        }
    }
}
