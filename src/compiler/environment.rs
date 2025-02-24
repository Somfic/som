use cranelift::prelude::{types, EntityRef, FunctionBuilder, Variable};
use std::{borrow::Cow, cell::Cell, collections::HashMap, rc::Rc};

use crate::ast::TypeValue;

pub struct CompileEnvironment<'env> {
    parent: Option<&'env CompileEnvironment<'env>>,
    variables: HashMap<Cow<'env, str>, Variable>,
    next_var: Rc<Cell<usize>>,
}

impl<'env> CompileEnvironment<'env> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            next_var: Rc::new(Cell::new(0)),
        }
    }

    pub fn declare(
        &mut self,
        name: Cow<'env, str>,
        builder: &mut FunctionBuilder,
        ty: &TypeValue,
    ) -> Variable {
        let var = Variable::new(self.next_var.get());
        self.next_var.set(self.next_var.get() + 1);
        builder.declare_var(var, Self::convert_type(ty));
        self.variables.insert(name, var);
        var
    }

    pub fn lookup(&self, name: &str) -> Option<&Variable> {
        self.variables
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup(name)))
    }

    // For handling new blocks, you can clone the current env or implement a more complex scoping system.
    pub fn block(&'env self) -> Self {
        Self {
            parent: Some(self),
            variables: self.variables.clone(),
            next_var: Rc::clone(&self.next_var),
        }
    }

    fn convert_type(ty: &TypeValue) -> types::Type {
        match ty {
            TypeValue::Integer => types::I64,
            TypeValue::Decimal => types::F64,
            TypeValue::Boolean => types::I8,
            _ => panic!("unsupported type"),
        }
    }
}
