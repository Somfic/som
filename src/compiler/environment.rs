use crate::ast::{
    Identifier, IntrinsicFunctionDeclaration, TypedFunctionDeclaration, Typing, TypingValue,
};
use cranelift::prelude::{EntityRef, FunctionBuilder, Signature, Variable};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};
use std::{borrow::Cow, cell::Cell, collections::HashMap, fmt::Display, rc::Rc};

pub struct CompileEnvironment<'ast> {
    parent: Option<&'ast CompileEnvironment<'ast>>,
    variables: HashMap<Cow<'ast, str>, Variable>,
    functions: HashMap<Cow<'ast, str>, (FuncId, Signature)>,
    types: HashMap<Cow<'ast, str>, Typing<'ast>>,
    next_variable: Rc<Cell<usize>>,
}

impl<'ast> CompileEnvironment<'ast> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            functions: HashMap::new(),
            types: HashMap::new(),
            next_variable: Rc::new(Cell::new(0)),
        }
    }

    pub fn declare_intrinsic(
        &mut self,
        function: &'ast IntrinsicFunctionDeclaration<'ast>,
        signature: Signature,
        module: &mut JITModule,
    ) -> FuncId {
        let func_id = module
            .declare_function(&function.identifier.name, Linkage::Export, &signature)
            .unwrap();

        self.functions
            .insert(function.identifier.name.clone(), (func_id, signature));

        func_id
    }

    pub fn declare_function(
        &mut self,
        function: &'ast TypedFunctionDeclaration<'ast>,
        signature: Signature,
        codebase: &mut JITModule,
    ) -> FuncId {
        let func_id = codebase
            .declare_function(&function.identifier.name, Linkage::Export, &signature)
            .unwrap();

        self.functions
            .insert(function.identifier.name.clone(), (func_id, signature));

        func_id
    }

    pub fn declare_variable(
        &mut self,
        identifier: Identifier<'ast>,
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
        identifier: Identifier<'ast>,
        builder: &mut FunctionBuilder,
        ty: Typing<'ast>,
    ) {
        self.types.insert(identifier.name, ty);
    }

    pub fn lookup_type(&self, identifier: &Identifier<'ast>) -> Option<&Typing<'ast>> {
        self.types
            .get(&identifier.name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_type(identifier)))
    }

    pub fn lookup_function(&self, identifier: &Identifier<'ast>) -> Option<&(FuncId, Signature)> {
        self.functions.get(&identifier.name).or_else(|| {
            self.parent
                .as_ref()
                .and_then(|p| p.lookup_function(identifier))
        })
    }

    pub fn lookup_variable(&self, identifier: &Identifier<'ast>) -> Option<&Variable> {
        self.variables.get(&identifier.name).or_else(|| {
            self.parent
                .as_ref()
                .and_then(|p| p.lookup_variable(identifier))
        })
    }

    pub fn block(&'ast self) -> Self {
        Self {
            parent: Some(self),
            variables: self.variables.clone(),
            types: self.types.clone(),
            functions: self.functions.clone(),
            next_variable: Rc::clone(&self.next_variable),
        }
    }
}
