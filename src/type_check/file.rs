use crate::{
    ast::{Declaration, File},
    Result, TypeCheck, TypeCheckContext, Typed, Untyped,
};

impl TypeCheck for File<Untyped> {
    type Output = File<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
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
