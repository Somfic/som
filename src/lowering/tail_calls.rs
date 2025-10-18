use crate::prelude::*;
use cranelift::prelude::*;
use cranelift_module::{FuncId, Module};

/// Metadata for tail-call optimized function compilation
pub struct TailCallContext {
    pub func_id: FuncId,
    pub loop_start: Block,
}

/// Analyzes a function body to detect if it contains self-recursive calls in tail position
pub fn is_tail_recursive(
    function_name: Option<&Identifier>,
    body: &TypedExpression,
) -> bool {
    let Some(name) = function_name else {
        return false;
    };

    has_tail_call(name, body)
}

/// Recursively checks if an expression contains a tail call to the given function
fn has_tail_call(function_name: &Identifier, expr: &TypedExpression) -> bool {
    match &expr.value {
        TypedExpressionValue::Call(call) => {
            // Check if this call is to the function we're looking for
            if is_call_to_function(function_name, &call.callee) {
                return true;
            }
            false
        }
        TypedExpressionValue::Conditional(cond) => {
            // Both branches are in tail position
            has_tail_call(function_name, &cond.truthy)
                || has_tail_call(function_name, &cond.falsy)
        }
        TypedExpressionValue::Block(block) => {
            // Only the final expression is in tail position
            has_tail_call(function_name, &block.result)
        }
        TypedExpressionValue::Group(group) => {
            has_tail_call(function_name, &group.expression)
        }
        _ => false,
    }
}

/// Checks if a callee expression is a call to the given function
fn is_call_to_function(function_name: &Identifier, callee: &TypedExpression) -> bool {
    match &callee.value {
        TypedExpressionValue::Identifier(id) => id.name == function_name.name,
        TypedExpressionValue::Group(group) => is_call_to_function(function_name, &group.expression),
        _ => false,
    }
}

/// Compiles a tail-recursive function with loop-based optimization
pub fn compile_tail_recursive_function(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    env: &mut CompileEnvironment,
    function_name: &Identifier,
) -> (
    FuncId,
    Vec<(String, cranelift::prelude::Variable, TypeValue)>,
) {
    use crate::lowering::captures::analyze_captures;
    use cranelift::codegen::ir::Function;
    use cranelift::prelude::{AbiParam, FunctionBuilderContext, Signature};

    let value = match &expression.value {
        TypedExpressionValue::Function(value) => value,
        _ => unreachable!(),
    };

    // Analyze captures first
    let captured_vars = analyze_captures(&value, env);

    // Create signature
    let mut signature = Signature::new(compiler.isa.default_call_conv());

    // Add captured variables as leading parameters
    for (_name, _var, ty) in &captured_vars {
        signature.params.push(AbiParam::new(ty.to_ir()));
    }

    // Add regular parameters
    for parameter in &value.parameters {
        signature
            .params
            .push(AbiParam::new(parameter.type_.value.to_ir()));
    }

    signature
        .returns
        .push(AbiParam::new(value.body.type_.value.to_ir()));

    // Create function ID early
    let func_id = compiler
        .codebase
        .declare_anonymous_function(&signature)
        .unwrap();

    // Pre-register for recursion if no captures
    if captured_vars.is_empty() {
        env.declare_function(function_name, func_id);
    }

    // Compile function body with tail-call optimization
    let mut context = compiler.codebase.make_context();
    context.func = Function::new();
    context.func.signature = signature;

    let mut function_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut context.func, &mut function_context);

    // Create entry block with parameters
    let entry_block = builder.create_block();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    // Create loop block for tail call optimization with parameters
    let loop_start = builder.create_block();

    // Add block parameters to loop_start for regular parameters (not captured variables)
    for parameter in value.parameters.iter() {
        builder.append_block_param(loop_start, parameter.type_.value.to_ir());
    }

    // Collect entry block parameters
    let entry_params = builder.block_params(entry_block).to_vec();

    // Jump to loop start, passing only the regular parameters (skip captured vars)
    let regular_param_start = captured_vars.len();
    let regular_params: Vec<cranelift::codegen::ir::BlockArg> = entry_params[regular_param_start..]
        .iter()
        .map(|v| cranelift::codegen::ir::BlockArg::Value(*v))
        .collect();
    builder.ins().jump(loop_start, &regular_params);
    builder.seal_block(entry_block);

    // Switch to loop block for function body
    builder.switch_to_block(loop_start);

    // Create a new environment for this function
    let mut function_env = env.block();

    // Bind captured variables from entry block parameters
    let mut param_index = 0;
    for (name, _var, ty) in &captured_vars {
        let variable = function_env.declare_variable(name, &mut builder, ty);
        builder.def_var(variable, entry_params[param_index]);
        param_index += 1;
    }

    // Bind regular parameters from loop_start block parameters
    let mut param_vars = Vec::new();
    let loop_params = builder.block_params(loop_start).to_vec();
    for (i, parameter) in value.parameters.iter().enumerate() {
        let variable = function_env.declare_variable(
            &parameter.identifier,
            &mut builder,
            &parameter.type_.value,
        );
        builder.def_var(variable, loop_params[i]);
        param_vars.push(variable);
    }

    // Store parameter variables for tail calls
    function_env.set_tail_call_params(param_vars);

    // Create tail call context
    let tail_ctx = TailCallContext {
        func_id,
        loop_start,
    };

    // Compile the body with tail call context
    let (body, tail_called) = compile_with_tail_call_context(
        compiler,
        &value.body,
        &mut builder,
        &mut function_env,
        &tail_ctx,
    );

    // Add return instruction only if we didn't perform a tail call
    if !tail_called {
        builder.ins().return_(&[body]);
    }

    builder.seal_block(loop_start);
    builder.finalize();

    if let Err(error) = compiler.codebase.define_function(func_id, &mut context) {
        println!("{:#?}", error);
    };

    (func_id, captured_vars)
}

/// Compiles an expression with tail-call context awareness
/// Returns (value, tail_called) where tail_called indicates if a tail call was performed
fn compile_with_tail_call_context(
    compiler: &mut Compiler,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
    tail_ctx: &TailCallContext,
) -> (Value, bool) {
    match &expression.value {
        TypedExpressionValue::Call(call) => {
            compile_call_with_tail_check(compiler, call, expression, body, env, tail_ctx)
        }
        TypedExpressionValue::Conditional(cond) => {
            compile_conditional_with_tail_propagation(compiler, cond, expression, body, env, tail_ctx)
        }
        TypedExpressionValue::Block(block) => {
            compile_block_with_tail_propagation(compiler, block, body, env, tail_ctx)
        }
        TypedExpressionValue::Group(group) => {
            compile_with_tail_call_context(compiler, &group.expression, body, env, tail_ctx)
        }
        _ => {
            // Not in tail position or not a tail-callable expression
            (compiler.compile_expression(expression, body, env), false)
        }
    }
}

/// Compiles a call, checking if it's a tail call that should be optimized
/// Returns (value, tail_called)
fn compile_call_with_tail_check(
    compiler: &mut Compiler,
    call: &crate::expressions::call::CallExpression<TypedExpression>,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
    tail_ctx: &TailCallContext,
) -> (Value, bool) {
    use crate::expressions;

    // Helper to extract identifier from callee expression
    fn get_callee_identifier(expr: &TypedExpression) -> Option<&Identifier> {
        match &expr.value {
            TypedExpressionValue::Identifier(id) => Some(id),
            TypedExpressionValue::Group(group) => get_callee_identifier(&group.expression),
            _ => None,
        }
    }

    fn get_func_id(
        compiler: &mut Compiler,
        expression: &TypedExpression,
        env: &mut CompileEnvironment,
    ) -> FuncId {
        match &expression.value {
            TypedExpressionValue::Function(_) => {
                let (func_id, _captured_vars) =
                    expressions::function::compile(compiler, expression, env);
                func_id
            }
            TypedExpressionValue::Group(group) => get_func_id(compiler, &group.expression, env),
            TypedExpressionValue::Identifier(identifier) => env.get_function(identifier).unwrap(),
            _ => panic!("not a function: {expression:?}"),
        }
    }

    let func_id = get_func_id(compiler, &call.callee, env);

    // Check if this is a tail call to the same function
    if func_id == tail_ctx.func_id {
        // This is a self-tail-call! Update parameters and jump instead of calling

        // Compile arguments to temporary values
        let arg_values: Vec<Value> = call
            .arguments
            .iter()
            .map(|arg| compiler.compile_expression(arg, body, env))
            .collect();

        // Jump back to loop start with new argument values
        let dummy = body.ins().iconst(cranelift::prelude::types::I64, 0);

        let block_args: Vec<cranelift::codegen::ir::BlockArg> =
            arg_values.iter().map(|v| cranelift::codegen::ir::BlockArg::Value(*v)).collect();
        body.ins().jump(tail_ctx.loop_start, &block_args);

        // Return dummy value (unreachable, but satisfies type system) and indicate tail call
        return (dummy, true);
    }

    // Not a tail call, do normal compilation
    (compiler.compile_expression(expression, body, env), false)
}

/// Compiles a conditional, propagating tail context to both branches
/// Returns (value, tail_called) where tail_called indicates if BOTH branches performed tail calls
fn compile_conditional_with_tail_propagation(
    compiler: &mut Compiler,
    cond: &crate::expressions::conditional::ConditionalExpression<TypedExpression>,
    expression: &TypedExpression,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
    tail_ctx: &TailCallContext,
) -> (Value, bool) {
    let condition_val = compiler.compile_expression(&cond.condition, body, env);

    let merge_block = body.create_block();
    let truthy_block = body.create_block();
    let falsy_block = body.create_block();

    body.append_block_param(merge_block, cond.truthy.type_.value.to_ir());

    body.ins()
        .brif(condition_val, truthy_block, &[], falsy_block, &[]);

    // Truthy branch - in tail position
    body.switch_to_block(truthy_block);
    let (truthy_val, truthy_tail_called) = compile_with_tail_call_context(compiler, &cond.truthy, body, env, tail_ctx);

    // Only jump to merge if we didn't perform a tail call
    if !truthy_tail_called {
        body.ins().jump(merge_block, &[cranelift::codegen::ir::BlockArg::Value(truthy_val)]);
    }
    body.seal_block(truthy_block);

    // Falsy branch - in tail position
    body.switch_to_block(falsy_block);
    let (falsy_val, falsy_tail_called) = compile_with_tail_call_context(compiler, &cond.falsy, body, env, tail_ctx);

    // Only jump to merge if we didn't perform a tail call
    if !falsy_tail_called {
        body.ins().jump(merge_block, &[cranelift::codegen::ir::BlockArg::Value(falsy_val)]);
    }
    body.seal_block(falsy_block);

    // Merge
    body.switch_to_block(merge_block);
    body.seal_block(merge_block);

    // If both branches performed tail calls, the merge block has no predecessors
    let both_tail_called = truthy_tail_called && falsy_tail_called;

    // Get the merge value
    let merge_val = if both_tail_called {
        // Both branches tail called, merge block unreachable - return dummy
        body.ins().iconst(cranelift::prelude::types::I64, 0)
    } else {
        // At least one branch didn't tail call, get the block param
        body.block_params(merge_block)[0]
    };

    (merge_val, both_tail_called)
}

/// Compiles a block, propagating tail context to the final expression
/// Returns (value, tail_called)
fn compile_block_with_tail_propagation(
    compiler: &mut Compiler,
    block: &crate::expressions::block::BlockExpression<TypedExpression>,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
    tail_ctx: &TailCallContext,
) -> (Value, bool) {
    let mut env = env.block();

    for statement in &block.statements {
        compiler.compile_statement(statement, body, &mut env);
    }

    // The final expression is in tail position
    compile_with_tail_call_context(compiler, &block.result, body, &mut env, tail_ctx)
}
