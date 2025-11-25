use crate::{
    ast::{Declaration, Expression, File, Visibility},
    Result, TypeCheck, TypeCheckContext, Typed, Untyped,
};

impl TypeCheck for File<Untyped> {
    type Output = File<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        // File-level pre-pass: collect all private lambdas for forward references
        for declaration in &self.declarations {
            if let Declaration::ValueDefinition(value_def) = declaration {
                // Only process private variables at file scope
                if matches!(value_def.visibility, Visibility::Private) {
                    // Only handle lambdas (functions) since we can infer their type without checking the body
                    if let Expression::Lambda(ref lambda) = *value_def.value {
                        let inferred_type = lambda.infer_type(ctx)?;
                        ctx.declare_variable(value_def.name.to_string(), inferred_type);
                    }
                }
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
            Declaration::ValueDefinition(value_definition) => {
                Declaration::ValueDefinition(value_definition.type_check(ctx)?)
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
