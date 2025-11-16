use crate::{
    ast::{Binary, Block, Expr, Expression, Group, Primary, Unary},
    parser::Untyped,
    type_check::{Type, TypeCheckContext, Typed},
    Result, TypeCheck, TypeCheckWithType,
};

impl TypeCheck for Expression<Untyped> {
    type Output = Expression<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let (expr, ty) = self.expr.type_check_with_type(ctx)?;

        Ok(Expression {
            expr,
            span: self.span.clone(),
            ty: ty.clone(),
        })
    }
}

impl TypeCheckWithType for Expression<Untyped> {
    type Output = Expression<Typed>;

    fn type_check_with_type(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        let (expr, ty) = self.expr.type_check_with_type(ctx)?;

        Ok((
            Expression {
                expr,
                span: self.span.clone(),
                ty: ty.clone(),
            },
            ty,
        ))
    }
}

impl TypeCheckWithType for Expr<Untyped> {
    type Output = Expr<Typed>;

    fn type_check_with_type(
        self,
        ctx: &mut TypeCheckContext,
    ) -> Result<(Self::Output, super::Type)> {
        match self {
            Expr::Primary(p) => p
                .type_check_with_type(ctx)
                .map(|(v, t)| (Expr::Primary(v), t)),
            Expr::Unary(u) => u
                .type_check_with_type(ctx)
                .map(|(v, t)| (Expr::Unary(v), t)),
            Expr::Binary(b) => b
                .type_check_with_type(ctx)
                .map(|(v, t)| (Expr::Binary(v), t)),
            Expr::Group(g) => g
                .type_check_with_type(ctx)
                .map(|(v, t)| (Expr::Group(v), t)),
            Expr::Block(g) => g
                .type_check_with_type(ctx)
                .map(|(v, t)| (Expr::Block(v), t)),
        }
    }
}

impl TypeCheckWithType for Primary {
    type Output = Primary;

    fn type_check_with_type(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        let ty = match &self {
            Primary::Boolean(_) => Type::Boolean,
            Primary::I32(_) => Type::I32,
            Primary::I64(_) => Type::I64,
            Primary::Decimal(_) => Type::Decimal,
            Primary::String(_) => Type::String,
            Primary::Character(_) => Type::Character,
            Primary::Identifier(i) => match ctx.get_variable(i.name.clone()) {
                Ok(t) => t,
                Err(e) => return Err(e), // todo: add span
            },
        };

        Ok((self, ty))
    }
}

impl TypeCheckWithType for Unary<Untyped> {
    type Output = Unary<Typed>;

    fn type_check_with_type(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        match self {
            Unary::Negate(expr) => expr
                .type_check_with_type(ctx)
                .map(|(v, t)| (Unary::Negate(Box::new(v)), t)),
        }
    }
}

impl TypeCheckWithType for Binary<Untyped> {
    type Output = Binary<Typed>;

    fn type_check_with_type(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        let (lhs, lhs_ty) = self.lhs.type_check_with_type(ctx)?;
        let (rhs, rhs_ty) = self.rhs.type_check_with_type(ctx)?;

        // todo: type check
        Ok((
            Binary {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                op: self.op,
            },
            lhs_ty,
        ))
    }
}

impl TypeCheckWithType for Group<Untyped> {
    type Output = Group<Typed>;

    fn type_check_with_type(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        self.expr
            .type_check_with_type(ctx)
            .map(|(v, t)| (Group { expr: Box::new(v) }, t))
    }
}

impl TypeCheckWithType for Block<Untyped> {
    type Output = Block<Typed>;

    fn type_check_with_type(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        let statements = self
            .statements
            .into_iter()
            .map(|s| s.type_check(ctx))
            .collect::<Result<Vec<_>>>()?;

        let (expression, ty) = match self.expression {
            Some(e) => {
                let (expr, ty) = e.type_check_with_type(ctx)?;
                (Some(Box::new(expr)), ty)
            }
            None => (None, Type::Unit),
        };

        Ok((
            Block {
                statements,
                expression,
            },
            ty,
        ))
    }
}
