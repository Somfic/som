use crate::ast::{Identifier, FunctionSignature, Typing, TypingValue};
use cranelift::prelude::{EntityRef, FunctionBuilder, Signature, Variable};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};
use std::{cell::Cell, collections::HashMap, rc::Rc};

pub struct CompileEnvironment<'parent> {
    parent: Option<&'parent CompileEnvironment<'parent>>,
    variables: HashMap<Box<str>, Variable>,
    functions: HashMap<Box<str>, (FuncId, Signature)>,
    types: HashMap<Box<str>, Typing>,
    next_variable: Rc<Cell<usize>>,
}

impl<'parent> CompileEnvironment<'parent> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            functions: HashMap::new(),
            types: HashMap::new(),
            next_variable: Rc::new(Cell::new(0)),
        }
    }

    pub fn declare_function(
        &mut self,
        identifier: Identifier,
        signature: Signature,
        codebase: &mut JITModule,
    ) -> FuncId {
        let func_id = codebase
            .declare_function(&identifier.name, Linkage::Export, &signature)
            .unwrap();

        self.functions
            .insert(identifier.name.clone(), (func_id, signature));

        func_id
    }

    pub fn declare_variable(
        &mut self,
        identifier: Identifier,
        builder: &mut FunctionBuilder,
        ty: &TypingValue,
    ) -> Variable {
        let var = Variable::new(self.next_variable.get());
        self.next_variable.set(self.next_variable.get() + 1);
        builder.declare_var(var, ty.to_ir());
        self.variables.insert(identifier.name, var);
        var
    }

    pub fn declare_type(
        &mut self,
        identifier: Identifier,
        builder: &mut FunctionBuilder,
        ty: Typing,
    ) {
        self.types.insert(identifier.name, ty);
    }

    pub fn lookup_type(&self, identifier: &Identifier) -> Option<&Typing> {
        self.types
            .get(&identifier.name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_type(identifier)))
    }

    pub fn lookup_function(&self, identifier: &Identifier) -> Option<&(FuncId, Signature)> {
        self.functions.get(&identifier.name).or_else(|| {
            self.parent
                .as_ref()
                .and_then(|p| p.lookup_function(identifier))
        })
    }

    pub fn lookup_variable(&self, identifier: &Identifier) -> Option<&Variable> {
        self.variables.get(&identifier.name).or_else(|| {
            self.parent
                .as_ref()
                .and_then(|p| p.lookup_variable(identifier))
        })
    }

    pub fn block(&'parent self) -> Self {
        Self {
            variables: self.variables.clone(),
            types: self.types.clone(),
            functions: self.functions.clone(),
            next_variable: Rc::clone(&self.next_variable),
            parent: Some(self),
        }
    }
}
