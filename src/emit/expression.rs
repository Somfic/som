use cranelift::{
    codegen::ir::BlockArg,
    prelude::{types, InstBuilder, IntCC, Value},
};

use crate::{
    ast::{
        Binary, BinaryOperation, Block, Expression, Group, Primary, PrimaryKind, Ternary, Unary,
        UnaryOperation,
    },
    Emit, EmitContext, Result, Type, TypeKind, Typed,
};

impl Emit for Expression<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        match self {
            Expression::Primary(p) => p.emit(ctx),
            Expression::Unary(u) => u.emit(ctx),
            Expression::Binary(b) => b.emit(ctx),
            Expression::Group(g) => g.emit(ctx),
            Expression::Block(b) => b.emit(ctx),
            Expression::Ternary(t) => t.emit(ctx),
        }
    }
}

impl Emit for Primary<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        Ok(match &self.kind {
            PrimaryKind::Boolean(b) => ctx
                .builder
                .ins()
                .iconst(TypeKind::Boolean.into(), if *b { 1 } else { 0 }),
            PrimaryKind::I32(i) => ctx.builder.ins().iconst(TypeKind::I32.into(), *i as i64),
            PrimaryKind::I64(i) => ctx.builder.ins().iconst(TypeKind::I64.into(), *i),
            PrimaryKind::Decimal(d) => ctx.builder.ins().f64const(*d),
            PrimaryKind::String(s) => unimplemented!("string emit"),
            PrimaryKind::Character(c) => unimplemented!("character emit"),
            PrimaryKind::Identifier(ident) => ident.emit(ctx)?,
        })
    }
}

impl Emit for Unary<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        let value = self.value.emit(ctx)?;

        match &self.op {
            UnaryOperation::Negate => Ok(ctx.builder.ins().ineg(value)),
        }
    }
}

impl Emit for Binary<Typed> {
    type Output = Value;

    fn emit(&self, ctx: &mut EmitContext) -> Result<Self::Output> {
        let lhs = self.lhs.emit(ctx)?;
        let rhs = self.rhs.emit(ctx)?;

        // TODO: support i32 vs i64 vs f64
        Ok(match &self.op {
            BinaryOperation::Add => ctx.builder.ins().iadd(lhs, rhs),
            BinaryOperation::Subtract => ctx.builder.ins().isub(lhs, rhs),
            BinaryOperation::Multiply => ctx.builder.ins().imul(lhs, rhs),
            BinaryOperation::Divide => ctx.builder.ins().fdiv(lhs, rhs),
            BinaryOperation::LessThan => ctx.builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs),
            BinaryOperation::LessThanOrEqual => {
                ctx.builder
                    .ins()
                    .icmp(IntCC::SignedLessThanOrEqual, lhs, rhs)
            }
            BinaryOperation::GreaterThan => {
                ctx.builder.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs)
            }
            BinaryOperation::GreaterThanOrEqual => {
                ctx.builder
                    .ins()
                    .icmp(IntCC::SignedGreaterThanOrEqual, lhs, rhs)
            }
            BinaryOperation::Equality => ctx.builder.ins().icmp(IntCC::Equal, lhs, rhs),
            BinaryOperation::Inequality => ctx.builder.ins().icmp(IntCC::NotEqual, lhs, rhs),
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
            .append_block_param(merge_block, self.truthy.ty().clone().into());

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
