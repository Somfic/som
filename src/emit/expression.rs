use cranelift::{
    codegen::ir::BlockArg,
    prelude::{types, InstBuilder, Value},
};

use crate::{
    ast::{Binary, BinaryOperation, Block, Expr, Expression, Group, Primary, Ternary, Unary},
    Emit, EmitContext, Result, Type, Typed,
};

impl Emit for Expression<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        self.expr.emit(ctx)
    }
}

impl Emit for Expr<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        match self {
            Expr::Primary(p) => p.emit(ctx),
            Expr::Unary(u) => u.emit(ctx),
            Expr::Binary(b) => b.emit(ctx),
            Expr::Group(g) => g.emit(ctx),
            Expr::Block(b) => b.emit(ctx),
            Expr::Ternary(t) => t.emit(ctx),
        }
    }
}

impl Emit for Primary {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        Ok(match self {
            Primary::Boolean(b) => ctx
                .builder
                .ins()
                .iconst(Type::Boolean.into(), if *b { 1 } else { 0 }),
            Primary::I32(i) => ctx.builder.ins().iconst(Type::I32.into(), *i as i64),
            Primary::I64(i) => ctx.builder.ins().iconst(Type::I64.into(), *i),
            Primary::Decimal(d) => ctx.builder.ins().f64const(*d),
            Primary::String(s) => unimplemented!("string emit"),
            Primary::Character(c) => unimplemented!("character emit"),
            Primary::Identifier(ident) => ident.emit(ctx)?,
        })
    }
}

impl Emit for Unary<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        match self {
            Unary::Negate(expression) => {
                let value = expression.emit(ctx)?;
                Ok(ctx.builder.ins().ineg(value))
            }
        }
    }
}

impl Emit for Binary<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        let lhs = self.lhs.emit(ctx)?;
        let rhs = self.rhs.emit(ctx)?;

        Ok(match &self.op {
            BinaryOperation::Add => ctx.builder.ins().iadd(lhs, rhs),
            BinaryOperation::Subtract => ctx.builder.ins().isub(lhs, rhs),
            BinaryOperation::Multiply => ctx.builder.ins().imul(lhs, rhs),
            BinaryOperation::Divide => ctx.builder.ins().fdiv(lhs, rhs),
        })
    }
}

impl Emit for Group<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        self.expr.emit(ctx)
    }
}

impl Emit for Block<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        for statement in &self.statements {
            statement.emit(ctx)?;
        }

        match &self.expression {
            Some(expression) => expression.emit(ctx),
            None => Ok(ctx.builder.ins().iconst(types::I8, 0)),
        }
    }
}

impl Emit for Ternary<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        let condition = self.condition.emit(ctx)?;

        let truthy_block = ctx.builder.create_block();
        let falsy_block = ctx.builder.create_block();
        let merge_block = ctx.builder.create_block();

        ctx.builder
            .append_block_param(merge_block, self.truthy.ty.clone().into());

        ctx.builder
            .ins()
            .brif(condition, truthy_block, &[], falsy_block, &[]);

        // truthy
        ctx.builder.switch_to_block(truthy_block);
        let value = self.truthy.emit(ctx)?;
        ctx.builder
            .ins()
            .jump(merge_block, &[BlockArg::Value(value)]);

        // falsy
        ctx.builder.switch_to_block(falsy_block);
        let value = self.falsy.emit(ctx)?;
        ctx.builder
            .ins()
            .jump(merge_block, &[BlockArg::Value(value)]);

        // merge
        ctx.builder.switch_to_block(merge_block);
        let value = ctx.builder.block_params(merge_block)[0];

        Ok(value)
    }
}
