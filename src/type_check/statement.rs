use crate::{
    ast::{Declaration, Scope, Statement},
    Result, TypeCheck, TypeCheckContext, Typed, Untyped,
};

impl TypeCheck for Statement<Untyped> {
    type Output = Statement<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        Ok(match self {
            Statement::Expression(e) => Statement::Expression(e.type_check(ctx)?),
            Statement::Scope(s) => Statement::Scope(s.type_check(ctx)?),
            Statement::Declaration(d) => Statement::Declaration(d.type_check(ctx)?),
        })
    }
}

impl TypeCheck for Scope<Untyped> {
    type Output = Scope<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        Ok(Scope {
            statements: self
                .statements
                .into_iter()
                .map(|s| s.type_check(ctx))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl TypeCheck for Declaration<Untyped> {
    type Output = Declaration<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let value = self.value.type_check(ctx)?;

        ctx.declare_variable(self.name.clone(), value.ty().clone());

        Ok(Declaration {
            name: self.name,
            value: Box::new(value),
        })
    }
}
