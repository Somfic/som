use crate::{
    ast::{Declaration, Expression, File, Visibility},
    Result, TypeCheck, TypeCheckContext, Typed, Untyped,
};

impl TypeCheck for File<Untyped> {
    type Output = File<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        // File-level pre-pass: collect all private functions and externs for forward references
        for declaration in &self.declarations {
            match declaration {
                Declaration::FunctionDefinition(func_def) => {
                    // Declare function definitions so they can be referenced by other declarations
                    if matches!(func_def.visibility, Visibility::Private) {
                        let function_type = crate::ast::FunctionType {
                            parameters: func_def.parameters.iter().map(|p| p.ty.clone()).collect(),
                            returns: Box::new(func_def.returns.clone()),
                            span: func_def.span.clone(),
                        };
                        ctx.declare_variable(
                            func_def.name.to_string(),
                            crate::ast::Type::Function(function_type),
                        );
                    }
                }
                Declaration::ExternDefinition(extern_def) => {
                    // Declare extern functions so they can be referenced by other declarations
                    for extern_func in &extern_def.functions {
                        ctx.declare_variable(
                            extern_func.name.to_string(),
                            crate::ast::Type::Function(extern_func.signature.clone()),
                        );
                    }
                }
                _ => {}
            }
        }

        // Now type check all declarations with forward references resolved
        let mut declarations = vec![];
        for declaration in self.declarations {
            declarations.push(declaration.type_check(ctx)?);
        }

        Ok(File { declarations })
    }
}

impl TypeCheck for Declaration<Untyped> {
    type Output = Declaration<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        Ok(match self {
            Declaration::Import(import) => Declaration::Import(import.type_check(ctx)?),
            Declaration::FunctionDefinition(function_definition) => {
                Declaration::FunctionDefinition(function_definition.type_check(ctx)?)
            }
            Declaration::TypeDefinition(type_definition) => {
                Declaration::TypeDefinition(type_definition.type_check(ctx)?)
            }
            Declaration::ExternDefinition(extern_definition) => {
                Declaration::ExternDefinition(extern_definition.type_check(ctx)?)
            }
        })
    }
}
