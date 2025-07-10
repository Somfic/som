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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationValue {
    Variable(cranelift::prelude::Variable),
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
            .insert(identifier.into(), DeclarationValue::Variable(var));
        builder.declare_var(var, ty.to_ir());
        var
    }

    pub fn get_variable(
        &self,
        identifier: impl Into<String>,
    ) -> Option<cranelift::prelude::Variable> {
        let name = identifier.into();
        if let Some(DeclarationValue::Variable(var)) = self.declarations.get(&name) {
            Some(*var)
        } else if let Some(parent) = self.parent {
            // recurse into parent scope
            parent.get_variable(name)
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
