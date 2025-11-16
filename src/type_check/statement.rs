use crate::{
    ast::{Block, Scope, Statement},
    Result, TypeCheck, TypeCheckContext, Typed, Untyped,
};

impl TypeCheck for Statement<Untyped> {
    type Output = Statement<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        Ok(match self {
            Statement::Expression(e) => Statement::Expression(e.type_check(ctx)?),
            Statement::Scope(s) => Statement::Scope(s.type_check(ctx)?),
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
