use inkwell::{
    context::Context,
    module::Module as LlvmModule,
    types::{BasicType, BasicTypeEnum},
};

use crate::ast::{Module, StatementValue, Type, TypeValue, TypedExpression, TypedStatement};

pub struct Compiler {
    llvm: Context,
}

impl<'ast> Compiler {
    pub fn new() -> Self {
        Self {
            llvm: Context::create(),
        }
    }

    pub fn compile(&self, modules: Vec<Module<'ast, TypedExpression<'ast>>>) {
        for module in modules {
            self.compile_module(module);
        }

        // module.add_function("test", self.context.bool_type().fn_type(param_types, is_var_args), linkage)
    }

    fn compile_module(&self, module: Module<'ast, TypedExpression<'ast>>) {
        let llvm_module = self.llvm.create_module(module.name.as_ref());
        for statement in module.definitions {
            self.compile_statement(&llvm_module, statement);
        }
    }

    fn compile_statement(&self, module: &LlvmModule, statement: TypedStatement<'ast>) {
        match statement.value {
            StatementValue::Function { header, body } => {
                let function = module.add_function(
                    header.name.as_ref(),
                    self.llvm.i32_type().fn_type(&[], false),
                    None,
                );

                let basic_block = self.llvm.append_basic_block(function, "entry");
                self.llvm.builder().position_at_end(basic_block);

                let return_value = body.codegen(&self.llvm, module);
                self.llvm.builder().build_return(Some(&return_value));
            }
            _ => todo!("compile_statement: {:?}", statement),
        }
    }
}

impl Type<'_> {
    pub fn to_llvm_type(self, llvm: &Context) -> Box<dyn BasicType> {
        match self.value {
            TypeValue::Symbol(_) => todo!(),
            TypeValue::Integer => Box::new(llvm.i32_type()),
            TypeValue::Unit => Box::new(llvm.void_type()),
        }
    }
}
