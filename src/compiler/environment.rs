use crate::ast::{IntrinsicFunctionDeclaration, TypedFunctionDeclaration, TypingValue};
use cranelift::prelude::{EntityRef, FunctionBuilder, Signature, Variable};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};
use std::{borrow::Cow, cell::Cell, collections::HashMap, fmt::Display, rc::Rc};

pub struct CompileEnvironment<'ast> {
    parent: Option<&'ast CompileEnvironment<'ast>>,
    variables: HashMap<Cow<'ast, str>, Variable>,
    functions: HashMap<Cow<'ast, str>, (FuncId, Signature)>,
    next_variable: Rc<Cell<usize>>,
}

impl<'ast> CompileEnvironment<'ast> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            functions: HashMap::new(),
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
            .declare_function(&function.name, Linkage::Export, &signature)
            .unwrap();

        self.functions
            .insert(function.name.clone(), (func_id, signature));

        func_id
    }

    pub fn declare_function(
        &mut self,
        function: &'ast TypedFunctionDeclaration<'ast>,
        signature: Signature,
        codebase: &mut JITModule,
    ) -> FuncId {
        let func_id = codebase
            .declare_function(&function.name, Linkage::Export, &signature)
            .unwrap();

        self.functions
            .insert(function.name.clone(), (func_id, signature));

        func_id
    }

    pub fn lookup_function(&self, name: &str) -> Option<&(FuncId, Signature)> {
        self.functions
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_function(name)))
    }

    pub fn declare_variable(
        &mut self,
        name: Cow<'ast, str>,
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

    pub fn block(&'ast self) -> Self {
        Self {
            parent: Some(self),
            variables: self.variables.clone(),
            functions: self.functions.clone(),
            next_variable: Rc::clone(&self.next_variable),
        }
    }
}

impl Display for CompileEnvironment<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // show all functions and variables in this scope, and the parent scopes, use indenting for each level

        let mut current_env: Option<&CompileEnvironment> = Some(self);
        let mut indent_level = 0;

        while let Some(env) = current_env {
            writeln!(f, "{:indent$}Functions:", "", indent = indent_level * 4)?;
            for (name, (func_id, _signature)) in &env.functions {
                writeln!(
                    f,
                    "{:indent$} - {} (FuncId: {:?})",
                    "",
                    name,
                    func_id,
                    indent = indent_level * 4
                )?;
            }

            writeln!(f, "{:indent$}Variables:", "", indent = indent_level * 4)?;
            for (name, var) in &env.variables {
                writeln!(
                    f,
                    "{:indent$} - {} (Variable: {:?})",
                    "",
                    name,
                    var,
                    indent = indent_level * 4
                )?;
            }

            current_env = env.parent;
            indent_level += 1;
        }

        Ok(())
    }
}
