use crate::{
    ast::{
        Declaration, Expression, ExternDefinition, Import, Scope, Statement, Type, TypeDefinition,
        ValueDefinition, WhileLoop,
    },
    expect_boolean, Result, TypeCheck, TypeCheckContext, Typed, Untyped,
};

impl TypeCheck for Statement<Untyped> {
    type Output = Statement<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        Ok(match self {
            Statement::Expression(e) => Statement::Expression(e.type_check(ctx)?),
            Statement::Scope(s) => Statement::Scope(s.type_check(ctx)?),
            Statement::ValueDefinition(d) => Statement::ValueDefinition(d.type_check(ctx)?),
            Statement::TypeDefinition(t) => Statement::TypeDefinition(t.type_check(ctx)?),
            Statement::ExternDefinition(e) => Statement::ExternDefinition(e.type_check(ctx)?),
            Statement::WhileLoop(w) => Statement::WhileLoop(w.type_check(ctx)?),
            Statement::Import(import) => Statement::Import(import.type_check(ctx)?),
        })
    }
}

impl TypeCheck for Scope<Untyped> {
    type Output = Scope<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        Ok(Scope {
            span: self.span,
            statements: self
                .statements
                .into_iter()
                .map(|s| s.type_check(ctx))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl TypeCheck for ValueDefinition<Untyped> {
    type Output = ValueDefinition<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        // if we're declaring a function, allow recursion by declaring it before type checking the value
        let value = if let Expression::Lambda(ref lambda) = *self.value {
            let ty = lambda.infer_type(ctx)?;
            ctx.declare_variable(self.name.clone(), ty);
            self.value.type_check(ctx)?
        } else {
            self.value.type_check(ctx)?
        };

        ctx.declare_variable(self.name.clone(), value.ty().clone());

        Ok(ValueDefinition {
            visibility: self.visibility,
            span: self.span,
            name: self.name,
            value: Box::new(value),
        })
    }
}

impl TypeCheck for TypeDefinition {
    type Output = TypeDefinition;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let resolved_ty = ctx.get_type_with_span(self.name.to_string(), &self.name.span)?;

        Ok(TypeDefinition {
            name: self.name,
            visibility: self.visibility,
            ty: resolved_ty,
            span: self.span,
        })
    }
}

impl TypeCheck for ExternDefinition {
    type Output = ExternDefinition;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        for function in &self.functions {
            ctx.declare_variable(
                function.name.clone(),
                Type::Function(function.signature.clone()),
            );
        }
        Ok(self)
    }
}

impl TypeCheck for WhileLoop<Untyped> {
    type Output = WhileLoop<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let condition = self.condition.type_check(ctx)?;

        expect_boolean(&condition.ty(), "while loop condition must be a boolean")?;

        let statement = self.statement.type_check(ctx)?;

        Ok(WhileLoop {
            span: self.span,
            condition,
            statement: Box::new(statement),
        })
    }
}

impl TypeCheck for Import {
    type Output = Import;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let scope = ctx.get_module_scope(&self.module)?;

        // Collect the types and variables first (cloning them)
        let types: Vec<(String, Type)> = scope
            .public_types
            .iter()
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect();

        let variables: Vec<(String, Type)> = scope
            .public_variables
            .iter()
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect();

        for (name, ty) in types {
            ctx.declare_type(name, ty);
        }

        for (name, ty) in variables {
            ctx.declare_variable(name, ty);
        }

        Ok(self)
    }
}
