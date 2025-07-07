use cranelift::prelude::EntityRef;
use cranelift_module::FuncId;

use crate::prelude::*;
use std::{cell::Cell, collections::HashMap, rc::Rc};

use crate::lexer::Identifier;

pub struct Environment<'env> {
    pub parent: Option<&'env Environment<'env>>,
    next_variable: Rc<Cell<usize>>,
}

impl<'env> Environment<'env> {
    pub fn new() -> Self {
        Self {
            parent: None,
            next_variable: Rc::new(Cell::new(0)),
        }
    }

    pub fn block(&'env self) -> Self {
        Environment {
            parent: Some(self),
            next_variable: self.next_variable.clone(),
        }
    }

    pub fn declare_variable(
        &mut self,
        builder: &mut FunctionBuilder,
        ty: &TypeValue,
    ) -> cranelift::prelude::Variable {
        let var = cranelift::prelude::Variable::new(self.next_variable.get());
        self.next_variable.set(self.next_variable.get() + 1);
        builder.declare_var(var, ty.to_ir());
        var
    }
}
