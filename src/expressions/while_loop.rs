use crate::{expressions, prelude::*};

#[derive(Debug, Clone, PartialEq)]
pub struct WhileLoopExpression<Expression> {
    pub condition: Box<Expression>,
    pub body: Box<Expression>,
}
pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let identifier = parser.expect(TokenKind::While, "expected a while loop")?;

    // Parse condition with Declaration binding power to prevent consuming the block
    let condition = parser.parse_expression(BindingPower::Declaration)?;
    let body = expressions::block::parse(parser)?;

    let span = identifier.span + body.span;

    Ok(ExpressionValue::WhileLoop(WhileLoopExpression {
        condition: Box::new(condition),
        body: Box::new(body),
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::WhileLoop(value) => value,
        _ => unreachable!(),
    };

    let condition = type_checker.check_expression(&value.condition, env);
    let body = type_checker.check_expression(&value.body, env);

    type_checker.expect_type_value(
        &condition.type_,
        &TypeValue::Boolean,
        "the condition should be a boolean",
    );

    let value = TypedExpressionValue::WhileLoop(WhileLoopExpression {
        condition: Box::new(condition),
        body: Box::new(body),
    });

    expression.with_value_type(value, TypeValue::Unit.with_span(expression.span))
}

pub fn compile(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> cranelift::prelude::Value {
    let value = match &expression.value {
        TypedExpressionValue::WhileLoop(value) => value,
        _ => unreachable!(),
    };

    let loop_condition = body.create_block();
    let loop_body = body.create_block();
    let loop_exit = body.create_block();

    // Jump from current block to loop condition
    body.ins().jump(loop_condition, &[]);

    // Condition block: evaluate condition and branch
    body.switch_to_block(loop_condition);
    let condition_val = compiler.compile_expression(&value.condition, body, env);
    body.ins()
        .brif(condition_val, loop_body, &[], loop_exit, &[]);

    // Loop body: compile the body and jump back to condition
    body.switch_to_block(loop_body);
    compiler.compile_expression(&value.body, body, env);
    body.ins().jump(loop_condition, &[]);
    body.seal_block(loop_body);

    // Now seal the condition block since all predecessors are known
    body.seal_block(loop_condition);

    // Exit block
    body.switch_to_block(loop_exit);
    body.seal_block(loop_exit);

    body.ins().iconst(cranelift::prelude::types::I8, 0)
}
