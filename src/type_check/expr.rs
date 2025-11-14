use crate::{
    ast::{Binary, Expr, Expression, Group, Primary, Unary},
    parser::ParsePhase,
    type_check::{Type, TypeCheck, TypeCheckContext},
    Result, TypeCheckPhase,
};

impl TypeCheck for Expression<ParsePhase> {
    type Output = Expression<TypeCheckPhase>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, super::Type)> {
        let (expr, ty) = self.expr.type_check(ctx)?;

        Ok((
            Expression {
                expr,
                span: self.span,
                ty: ty.clone(),
            },
            ty,
        ))
    }
}

impl TypeCheck for Expr<ParsePhase> {
    type Output = Expr<TypeCheckPhase>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, super::Type)> {
        match self {
            Expr::Primary(p) => p.type_check(ctx).map(|(v, t)| (Expr::Primary(v), t)),
            Expr::Unary(u) => u.type_check(ctx).map(|(v, t)| (Expr::Unary(v), t)),
            Expr::Binary(b) => b.type_check(ctx).map(|(v, t)| (Expr::Binary(v), t)),
            Expr::Group(g) => g.type_check(ctx).map(|(v, t)| (Expr::Group(v), t)),
        }
    }
}

impl TypeCheck for Primary {
    type Output = Primary;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        let ty = match &self {
            Primary::Boolean(_) => Type::Boolean,
            Primary::I32(_) => Type::I32,
            Primary::I64(_) => Type::I64,
            Primary::Decimal(_) => Type::Decimal,
            Primary::String(_) => Type::String,
            Primary::Character(_) => Type::Character,
            Primary::Identifier(i) => todo!(),
        };

        Ok((self, ty))
    }
}

impl TypeCheck for Unary<ParsePhase> {
    type Output = Unary<TypeCheckPhase>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        match self {
            Unary::Negate(expr) => expr
                .type_check(ctx)
                .map(|(v, t)| (Unary::Negate(Box::new(v)), t)),
        }
    }
}

impl TypeCheck for Binary<ParsePhase> {
    type Output = Binary<TypeCheckPhase>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        let (lhs, lhs_ty) = self.lhs.type_check(ctx)?;
        let (rhs, rhs_ty) = self.rhs.type_check(ctx)?;

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

impl TypeCheck for Group<ParsePhase> {
    type Output = Group<TypeCheckPhase>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<(Self::Output, Type)> {
        self.expr
            .type_check(ctx)
            .map(|(v, t)| (Group { expr: Box::new(v) }, t))
    }
}
