use cranelift::codegen::ir::BlockArg;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalExpression<Expression> {
    pub condition: Box<Expression>,
    pub truthy: Box<Expression>,
    pub falsy: Box<Expression>,
}

pub fn parse(
    parser: &mut Parser,
    lhs: Expression,
    binding_power: BindingPower,
) -> Result<Expression> {
    parser.expect(TokenKind::If, "expected a condition")?;

    let condition = parser.parse_expression(BindingPower::None)?;

    parser.expect(TokenKind::Else, "expected a falsy branch")?;

    let falsy = parser.parse_expression(BindingPower::None)?;

    let span = lhs.span + condition.span + falsy.span;

    let value = ExpressionValue::Conditional(ConditionalExpression {
        condition: Box::new(condition),
        truthy: Box::new(lhs),
        falsy: Box::new(falsy),
    });

    Ok(value.with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Conditional(value) => value,
        _ => unreachable!(),
    };

    let condition = type_checker.check_expression(&value.condition, env);
    let truthy = type_checker.check_expression(&value.truthy, env);
    let falsy = type_checker.check_expression(&value.falsy, env);

    type_checker.expect_type_value(
        &condition.type_,
        &TypeValue::Boolean,
        "the condition should be a boolean",
    );

    type_checker.expect_same_type(
        vec![&truthy.type_, &falsy.type_],
        "the truthy and falsy branches should have the same type",
    );

    let type_ = truthy.type_.clone().with_span(expression.span);

    let value = TypedExpressionValue::Conditional(ConditionalExpression {
        condition: Box::new(condition),
        truthy: Box::new(truthy),
        falsy: Box::new(falsy),
    });

    expression.with_value_type(value, type_)
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
    tail_ctx: crate::compiler::TailContext,
) -> CompileValue {
    let value = match &expression.value {
        TypedExpressionValue::Conditional(value) => value,
        _ => unreachable!(),
    };

    let condition_val = compiler.compile_expression(&value.condition, body, env);

    let merge_block = body.create_block();
    let truthy_block = body.create_block();
    let falsy_block = body.create_block();

    body.append_block_param(merge_block, value.truthy.type_.value.to_ir());

    body.ins()
        .brif(condition_val, truthy_block, &[], falsy_block, &[]);

    // truthy - both branches are in tail position if conditional is
    body.switch_to_block(truthy_block);
    let is_truthy_tail_call = matches!(&value.truthy.value, TypedExpressionValue::Call(_)) && matches!(tail_ctx, crate::compiler::TailContext::InTail { .. });
    let truthy_val = compiler.compile_expression_with_tail(&value.truthy, body, env, tail_ctx);
    if !is_truthy_tail_call {
        body.ins().jump(merge_block, &[BlockArg::Value(truthy_val)]);
    }
    body.seal_block(truthy_block);

    // falsy - both branches are in tail position if conditional is
    body.switch_to_block(falsy_block);
    let is_falsy_tail_call = matches!(&value.falsy.value, TypedExpressionValue::Call(_)) && matches!(tail_ctx, crate::compiler::TailContext::InTail { .. });
    let falsy_val = compiler.compile_expression_with_tail(&value.falsy, body, env, tail_ctx);
    if !is_falsy_tail_call {
        body.ins().jump(merge_block, &[BlockArg::Value(falsy_val)]);
    }
    body.seal_block(falsy_block);

    // merge
    body.switch_to_block(merge_block);
    body.seal_block(merge_block);
    body.block_params(merge_block)[0]
}
