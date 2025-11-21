use crate::{
    ast::{
        Binary, BinaryOperation, Block, Call, Construction, Expression, FieldAccess, Group, Lambda,
        Primary, PrimaryKind, Pseudo, Ternary, Type, Unary,
    },
    parser::Untyped,
    type_check::{TypeCheckContext, Typed},
    Result, TypeCheck, TypeCheckError,
};

impl TypeCheck for Expression<Untyped> {
    type Output = Expression<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        match self {
            Expression::Primary(p) => p.type_check(ctx).map(Expression::Primary),
            Expression::Unary(u) => u.type_check(ctx).map(Expression::Unary),
            Expression::Binary(b) => b.type_check(ctx).map(Expression::Binary),
            Expression::Group(g) => g.type_check(ctx).map(Expression::Group),
            Expression::Block(b) => b.type_check(ctx).map(Expression::Block),
            Expression::Ternary(t) => t.type_check(ctx).map(Expression::Ternary),
            Expression::Lambda(l) => l.type_check(ctx).map(Expression::Lambda),
            Expression::Call(c) => c.type_check(ctx).map(Expression::Call),
            Expression::Construction(c) => c.type_check(ctx).map(Expression::Construction),
            Expression::FieldAccess(f) => f.type_check(ctx).map(Expression::FieldAccess),
        }
    }
}

impl TypeCheck for Primary<Untyped> {
    type Output = Primary<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let ty = match &self.kind {
            PrimaryKind::Boolean(_) => Type::boolean(self.span.clone()),
            PrimaryKind::I32(_) => Type::i32(self.span.clone()),
            PrimaryKind::I64(_) => Type::i64(self.span.clone()),
            PrimaryKind::Decimal(_) => Type::decimal(self.span.clone()),
            PrimaryKind::String(_) => Type::string(self.span.clone()),
            PrimaryKind::Character(_) => Type::character(self.span.clone()),
            PrimaryKind::Identifier(i) => match ctx.get_variable(i.name.clone()) {
                Ok(ty) => Ok(ty.with_span(&self.span)),
                Err(err) => err
                    .with_label(self.span.label("could not be found"))
                    .to_err(),
            }?,
        };

        Ok(Primary {
            ty,
            kind: self.kind,
            span: self.span,
        })
    }
}

impl TypeCheck for Unary<Untyped> {
    type Output = Unary<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let value = self.value.type_check(ctx)?;
        let ty = value.ty().clone();

        Ok(Unary {
            op: self.op,
            value: Box::new(value),
            span: self.span,
            ty,
        })
    }
}

impl TypeCheck for Binary<Untyped> {
    type Output = Binary<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let display = self.to_string();
        let lhs = self.lhs.type_check(ctx)?;
        let rhs = self.rhs.type_check(ctx)?;

        expect_type(
            lhs.ty(),
            rhs.ty(),
            format!(
                "{} requires both sides to be of the same type, but {} and {} do not match",
                display,
                lhs.ty(),
                rhs.ty()
            ),
        )?;

        let ty = match self.op {
            BinaryOperation::Add
            | BinaryOperation::Subtract
            | BinaryOperation::Multiply
            | BinaryOperation::Divide => lhs.ty().with_span(&(lhs.span() + rhs.span())),
            BinaryOperation::LessThan
            | BinaryOperation::LessThanOrEqual
            | BinaryOperation::GreaterThan
            | BinaryOperation::GreaterThanOrEqual
            | BinaryOperation::Equality
            | BinaryOperation::Inequality => Type::boolean(lhs.span() + rhs.span()),
        };

        Ok(Binary {
            span: lhs.span() + rhs.span(),
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            op: self.op,
            ty,
        })
    }
}

impl TypeCheck for Group<Untyped> {
    type Output = Group<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        self.expr.type_check(ctx).map(|v| Group {
            ty: v.ty().clone(),
            expr: Box::new(v),
            span: self.span,
        })
    }
}

impl TypeCheck for Block<Untyped> {
    type Output = Block<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let statements = self
            .statements
            .into_iter()
            .map(|s| s.type_check(ctx))
            .collect::<Result<Vec<_>>>()?;

        let expression = match self.expression {
            Some(e) => Some(Box::new(e.type_check(ctx)?)),
            None => None,
        };

        let ty = match &expression {
            Some(e) => e.ty().clone(),
            None => Type::unit(self.span.clone()),
        };

        Ok(Block {
            statements,
            expression,
            span: self.span,
            ty,
        })
    }
}

impl TypeCheck for Ternary<Untyped> {
    type Output = Ternary<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let condition = self.condition.type_check(ctx)?;
        let truthy = self.truthy.type_check(ctx)?;
        let falsy = self.falsy.type_check(ctx)?;

        expect_boolean(
            condition.ty(),
            "the condition of a ternary must be a boolean",
        )?;
        expect_type(
            truthy.ty(),
            falsy.ty(),
            "the two branches of a ternary must be the same",
        )?;

        Ok(Ternary {
            ty: truthy.ty().clone(),
            span: self.span,
            condition: Box::new(condition),
            truthy: Box::new(truthy),
            falsy: Box::new(falsy),
        })
    }
}

impl TypeCheck for Lambda<Untyped> {
    type Output = Lambda<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let mut lambda_ctx = ctx.new_child_context();
        for param in &self.parameters {
            lambda_ctx.declare_variable(param.name.name.to_string(), param.ty.clone());
        }

        let body = self.body.type_check(&mut lambda_ctx)?;

        if let Some(return_ty) = &self.explicit_return_ty {
            expect_type(return_ty, body.ty(), format!("{} was provided as the function's return type, but the function actually returns {}, which does not match", return_ty, body.ty()))?;
        }

        let parameter_types: Vec<_> = self.parameters.iter().map(|p| p.ty.clone()).collect();

        Ok(Lambda {
            id: self.id,
            parameters: self.parameters,
            explicit_return_ty: self.explicit_return_ty,
            ty: Type::function(
                parameter_types,
                Box::new(body.ty().clone()),
                self.span.clone(),
            ),
            span: self.span,
            body: Box::new(body),
        })
    }
}

impl Lambda<Untyped> {
    pub fn infer_type(&self, ctx: &mut TypeCheckContext) -> Result<Type> {
        let parameter_types: Vec<_> = self.parameters.iter().map(|p| p.ty.clone()).collect();
        let returns = self
            .explicit_return_ty
            .clone()
            .unwrap_or_else(|| Type::unit(self.span.clone()));

        Ok(Type::function(
            parameter_types,
            Box::new(returns),
            self.span.clone(),
        ))
    }
}

impl TypeCheck for Call<Untyped> {
    type Output = Call<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let callee = self.callee.type_check(ctx)?;

        let (parameter_types, return_type) = match callee.ty() {
            Type::Function(f) => (&f.parameters, &f.returns),
            _ => {
                return TypeCheckError::NotAFunction
                    .to_diagnostic()
                    .with_label(callee.span().label("not a function"))
                    .with_hint(format!(
                        "{} is not a function and thus cannot be called",
                        callee.ty()
                    ))
                    .to_err();
            }
        };

        if self.arguments.len() != parameter_types.len() {
            return TypeCheckError::ArgumentCountMismatch
                .to_diagnostic()
                .with_label(self.span.label(format!(
                    "expected {} arguments, got {}",
                    parameter_types.len(),
                    self.arguments.len()
                )))
                .to_err();
        }

        let arguments: Vec<_> = self
            .arguments
            .into_iter()
            .zip(parameter_types.iter())
            .map(|(arg, expected_ty)| {
                let arg = arg.type_check(ctx)?;
                expect_type(arg.ty(), expected_ty, format!("argument type mismatch"))?;
                Ok(arg)
            })
            .collect::<Result<_>>()?;

        Ok(Call {
            ty: (**return_type).clone(),
            callee: Box::new(callee),
            arguments,
            span: self.span.clone(),
        })
    }
}

impl TypeCheck for Construction<Untyped> {
    type Output = Construction<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let struct_ty = match ctx.get_type(self.struct_ty.clone()) {
            Ok(ty) => ty,
            Err(err) => {
                return err
                    .with_label(self.struct_ty.span.label("could not be found"))
                    .to_err();
            }
        };

        expect_struct(
            &struct_ty,
            format!(
                "{} is not a struct and thus cannot be constructed",
                struct_ty
            ),
        )?;

        let mut typed_fields = Vec::new();
        for (name, expr) in self.fields {
            let typed_expr = expr.type_check(ctx)?;
            typed_fields.push((name, typed_expr));
        }

        Ok(Construction {
            ty: struct_ty.with_span(&self.span),
            struct_ty: self.struct_ty,
            fields: typed_fields,
            span: self.span,
        })
    }
}

fn expect_type(a: &Type, b: &Type, hint: impl Into<String>) -> Result<()> {
    if a != b {
        return Err(TypeCheckError::TypeMismatch
            .to_diagnostic()
            .with_label(a.span().label(format!("{}", a.pseudo())))
            .with_label(b.span().label(format!("{}", b.pseudo())))
            .with_hint(hint.into()));
    }

    Ok(())
}

fn expect_boolean(actual: &Type, hint: impl Into<String>) -> Result<()> {
    if !matches!(actual, Type::Boolean(_)) {
        return TypeCheckError::ExpectedType
            .to_diagnostic()
            .with_label(
                actual
                    .span()
                    .label(format!("{}, expected a boolean", actual)),
            )
            .with_hint(hint.into())
            .to_err();
    }
    Ok(())
}

fn expect_struct(actual: &Type, hint: impl Into<String>) -> Result<()> {
    if !matches!(actual, Type::Struct(_)) {
        return TypeCheckError::ExpectedType
            .to_diagnostic()
            .with_label(
                actual
                    .span()
                    .label(format!("{}, expected a struct", actual)),
            )
            .with_hint(hint.into())
            .to_err();
    }
    Ok(())
}

impl TypeCheck for FieldAccess<Untyped> {
    type Output = FieldAccess<Typed>;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        let object = self.object.type_check(ctx)?;

        let struct_ty = match object.ty() {
            Type::Struct(s) => s,
            _ => {
                return TypeCheckError::ExpectedStruct
                    .to_diagnostic()
                    .with_label(object.span().label("not a struct"))
                    .with_hint(format!(
                        "{} is not a struct and thus has no fields",
                        object.ty()
                    ))
                    .to_err();
            }
        };

        let struct_name = match &struct_ty.name {
            Some(struct_name) => struct_name,
            None => {
                return TypeCheckError::ExpectedStruct
                    .to_diagnostic()
                    .with_label(object.span().label("not a struct"))
                    .with_hint(format!(
                        "{} is not a struct and thus has no fields",
                        object.ty()
                    ))
                    .to_err();
            }
        };

        let field_ty = match struct_ty.fields.iter().find_map(|field| {
            if field.name == self.field {
                Some(field.ty.clone())
            } else {
                None
            }
        }) {
            Some(ty) => ty,
            None => {
                return TypeCheckError::ExpectedField
                    .to_diagnostic()
                    .with_label(self.span.label(format!(
                        "struct '{}' has no field named '{}'",
                        struct_name, self.field
                    )))
                    .with_hint(format!(
                        "the struct '{}' does not contain a field named '{}'",
                        struct_name, self.field
                    ))
                    .to_err();
            }
        };

        Ok(FieldAccess {
            ty: field_ty.with_span(&self.span),
            object: Box::new(object),
            field: self.field,
            span: self.span,
        })
    }
}
