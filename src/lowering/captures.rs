use crate::prelude::*;
use std::collections::HashSet;

/// Analyze what variables a function captures from its environment
pub fn analyze_captures(
    function: &crate::expressions::function::FunctionExpression<TypedExpression>,
    env: &CompileEnvironment,
) -> Vec<(String, cranelift::prelude::Variable, TypeValue)> {
    let mut captured = Vec::new();
    let mut local_vars = HashSet::new();

    // Add parameters to local variables
    for param in &function.parameters {
        local_vars.insert(param.identifier.name.to_string());
    }

    // Find all identifiers used in the function body
    collect_captures_from_expr(&function.body, &mut captured, &local_vars, env);

    captured
}

fn collect_captures_from_expr(
    expr: &TypedExpression,
    captured: &mut Vec<(String, cranelift::prelude::Variable, TypeValue)>,
    local_vars: &HashSet<String>,
    env: &CompileEnvironment,
) {
    match &expr.value {
        TypedExpressionValue::Identifier(identifier) => {
            let name = identifier.name.to_string();
            // If it's not a local variable and we haven't already captured it
            if !local_vars.contains(&name) && !captured.iter().any(|(n, _, _)| n == &name) {
                // Try to get it from the environment
                if let Some((var, ty)) = env.get_variable_with_type(&name) {
                    captured.push((name, var, ty.clone()));
                }
            }
        }
        TypedExpressionValue::Block(block) => {
            let mut block_locals = local_vars.clone();
            for stmt in &block.statements {
                if let StatementValue::VariableDeclaration(decl) = &stmt.value {
                    block_locals.insert(decl.identifier.name.to_string());
                    collect_captures_from_expr(&decl.value, captured, &block_locals, env);
                }
            }
            collect_captures_from_expr(&block.result, captured, &block_locals, env);
        }
        TypedExpressionValue::Binary(binary) => {
            collect_captures_from_expr(&binary.left, captured, local_vars, env);
            collect_captures_from_expr(&binary.right, captured, local_vars, env);
        }
        TypedExpressionValue::Unary(unary) => {
            collect_captures_from_expr(&unary.operand, captured, local_vars, env);
        }
        TypedExpressionValue::Call(call) => {
            collect_captures_from_expr(&call.callee, captured, local_vars, env);
            for arg in &call.arguments {
                collect_captures_from_expr(arg, captured, local_vars, env);
            }
        }
        TypedExpressionValue::Conditional(cond) => {
            collect_captures_from_expr(&cond.condition, captured, local_vars, env);
            collect_captures_from_expr(&cond.truthy, captured, local_vars, env);
            collect_captures_from_expr(&cond.falsy, captured, local_vars, env);
        }
        TypedExpressionValue::Function(_) => {
            // Nested functions would need their own analysis
            // For now, skip them
        }
        TypedExpressionValue::Group(group) => {
            collect_captures_from_expr(&group.expression, captured, local_vars, env);
        }
        TypedExpressionValue::Assignment(assignment) => {
            collect_captures_from_expr(&assignment.value, captured, local_vars, env);
        }
        TypedExpressionValue::FieldAccess(field) => {
            collect_captures_from_expr(&field.object, captured, local_vars, env);
        }
        TypedExpressionValue::StructConstructor(strukt) => {
            for arg in &strukt.arguments {
                collect_captures_from_expr(&arg.value, captured, local_vars, env);
            }
        }
        TypedExpressionValue::WhileLoop(while_loop) => {
            collect_captures_from_expr(&while_loop.condition, captured, local_vars, env);
            collect_captures_from_expr(&while_loop.body, captured, local_vars, env);
        }
        TypedExpressionValue::Primary(_) => {
            // Literals don't capture anything
        }
    }
}
