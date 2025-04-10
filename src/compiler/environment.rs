use crate::ast::{TypedFunctionDeclaration, TypingValue};
use cranelift::prelude::{
        EntityRef, FunctionBuilder, Signature, Variable,
    };
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};
use std::{borrow::Cow, cell::Cell, collections::HashMap, rc::Rc};

pub struct CompileEnvironment<'env> {
    parent: Option<&'env CompileEnvironment<'env>>,
    variables: HashMap<Cow<'env, str>, Variable>,
    functions: HashMap<Cow<'env, str>, (FuncId, Signature, &'env TypedFunctionDeclaration<'env>)>,
    next_variable: Rc<Cell<usize>>,
}

impl<'env> CompileEnvironment<'env> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            functions: HashMap::new(),
            next_variable: Rc::new(Cell::new(0)),
        }
    }

    pub fn declare_function<'ast>(
        &mut self,
        function: &'env TypedFunctionDeclaration<'ast>,
        signature: Signature,
        module: &mut JITModule,
    ) -> FuncId {
        let func_id = module
            .declare_function(&function.name, Linkage::Export, &signature)
            .unwrap();

        self.functions
            .insert(function.name.clone(), (func_id, signature, function));

        func_id
    }

    pub fn lookup_function(
        &self,
        name: &str,
    ) -> Option<&(FuncId, Signature, &'env TypedFunctionDeclaration<'env>)> {
        self.functions
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_function(name)))
    }

    pub fn declare_variable(
        &mut self,
        name: Cow<'env, str>,
        builder: &mut FunctionBuilder,
        ty: &TypingValue,
    ) -> Variable {
        let var = Variable::new(self.next_variable.get());
        self.next_variable.set(self.next_variable.get() + 1);
        builder.declare_var(var, super::convert_type(ty));
        self.variables.insert(name, var);
        var
    }

    pub fn lookup_variable(&self, name: &str) -> Option<&Variable> {
        self.variables
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_variable(name)))
    }

    pub fn block(&'env self) -> Self {
        Self {
            parent: Some(self),
            variables: self.variables.clone(),
            functions: self.functions.clone(),
            next_variable: Rc::clone(&self.next_variable),
        }
    }
}
