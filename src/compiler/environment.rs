use super::Compiler;
use crate::ast::TypeValue;
use cranelift::{
    codegen::ir::UserFuncName,
    prelude::{EntityRef, FunctionBuilder, Variable},
};
use std::{borrow::Cow, cell::Cell, collections::HashMap, rc::Rc};

pub struct CompileEnvironment<'env> {
    parent: Option<&'env CompileEnvironment<'env>>,
    variables: HashMap<Cow<'env, str>, Variable>,
    functions: HashMap<Cow<'env, str>, u32>,
    next_variable: Rc<Cell<usize>>,
    next_function: Rc<Cell<u32>>,
}

impl<'env> CompileEnvironment<'env> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            functions: HashMap::new(),
            next_variable: Rc::new(Cell::new(0)),
            next_function: Rc::new(Cell::new(0)),
        }
    }

    pub fn declare_function<'ast>(
        &mut self,
        compiler: &'ast mut Compiler,
        name: Cow<'env, str>,
    ) -> FunctionBuilder<'ast> {
        compiler.jit.ctx.func.name = UserFuncName::user(0, self.next_function.get());
        self.functions.insert(name, self.next_function.get());
        self.next_function.set(self.next_function.get() + 1);

        let builder_context = &mut compiler.jit.builder_context;
        FunctionBuilder::new(&mut compiler.jit.ctx.func, builder_context)
    }

    pub fn lookup_function(&self, name: &str) -> Option<&u32> {
        self.functions
            .get(name)
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_function(name)))
    }

    pub fn declare_variable(
        &mut self,
        name: Cow<'env, str>,
        builder: &mut FunctionBuilder,
        ty: &TypeValue,
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
            next_function: Rc::clone(&self.next_function),
        }
    }
}
