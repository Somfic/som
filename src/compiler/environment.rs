use cranelift::prelude::EntityRef;
use cranelift_module::FuncId;

use crate::prelude::*;
use std::{cell::Cell, collections::HashMap, rc::Rc};

use crate::lexer::Identifier;

pub struct Environment<'env> {
    pub parent: Option<&'env Environment<'env>>,
    pub declarations: HashMap<Identifier, DeclarationValue>,
    next_variable: Rc<Cell<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclarationValue {
    Variable(cranelift::prelude::Variable),
    Function(FuncId),
}

impl<'env> Environment<'env> {
    pub fn new() -> Self {
        Self {
            parent: None,
            next_variable: Rc::new(Cell::new(0)),
            declarations: HashMap::new(),
        }
    }

    pub fn block(&'env self) -> Self {
        Environment {
            parent: Some(self),
            next_variable: self.next_variable.clone(),
            // todo: inherit from parent?
            declarations: self.declarations.clone(),
        }
    }

    pub fn declare_function(&mut self, identifier: &Identifier, func_id: FuncId) -> FuncId {
        self.declarations
            .insert(identifier.clone(), DeclarationValue::Function(func_id));

        func_id
    }

    pub fn declare_variable(
        &mut self,
        identifier: &Identifier,
        builder: &mut FunctionBuilder,
        ty: &TypeValue,
    ) -> cranelift::prelude::Variable {
        let var = cranelift::prelude::Variable::new(self.next_variable.get());
        self.next_variable.set(self.next_variable.get() + 1);
        self.declarations
            .insert(identifier.clone(), DeclarationValue::Variable(var));
        builder.declare_var(var, ty.to_ir());
        var
    }

    pub fn get_variable(&self, identifier: &Identifier) -> Option<cranelift::prelude::Variable> {
        self.declarations
            .get(identifier)
            .and_then(|value| match value {
                DeclarationValue::Variable(var) => Some(*var),
                _ => None,
            })
    }

    pub fn get_function(&self, identifier: &Identifier) -> Option<FuncId> {
        self.declarations
            .get(identifier)
            .and_then(|value| match value {
                DeclarationValue::Function(func_id) => Some(*func_id),
                _ => None,
            })
    }
}
