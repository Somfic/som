use crate::expressions::function::FunctionExpression;
use crate::prelude::*;
use crate::types::closure::{CaptureAnalysis, CaptureMode, CapturedVariable};

/// Analyzer for determining what variables a function captures from its environment
pub struct CaptureAnalyzer<'env> {
    /// The type environment to look up variable declarations
    type_env: &'env TypeEnvironment<'env>,
    /// Current scope depth (0 = current function scope, 1 = parent scope, etc.)
    scope_depth: usize,
}

impl<'env> CaptureAnalyzer<'env> {
    pub fn new(type_env: &'env TypeEnvironment<'env>) -> Self {
        Self {
            type_env,
            scope_depth: 0,
        }
    }

    /// Analyze a function expression to determine what variables it captures
    pub fn analyze_function(
        &mut self,
        function_expr: &FunctionExpression<TypedExpression>,
    ) -> CaptureAnalysis {
        let mut analysis = CaptureAnalysis::new();

        // Create a new scope for the function parameters
        let mut function_env = self.type_env.function();

        // Add parameters to the function scope
        for parameter in &function_expr.parameters {
            function_env.declare(&parameter.identifier, &parameter.type_);
            analysis.add_local_variable(
                parameter.identifier.name.to_string(),
                parameter.type_.value.clone(),
            );
        }

        // Analyze the function body with the new environment
        let analyzer = CaptureAnalyzer {
            type_env: &function_env,
            scope_depth: self.scope_depth + 1,
        };

        analyzer.analyze_expression(&function_expr.body, &mut analysis);

        analysis
    }

    /// Analyze an expression to find captured variables
    fn analyze_expression(&self, expr: &TypedExpression, analysis: &mut CaptureAnalysis) {
        match &expr.value {
            TypedExpressionValue::Identifier(identifier) => {
                self.analyze_identifier(identifier, analysis);
            }
            TypedExpressionValue::Function(function_expr) => {
                // Nested function - recursively analyze
                let mut nested_analyzer = CaptureAnalyzer {
                    type_env: self.type_env,
                    scope_depth: self.scope_depth,
                };
                let nested_analysis = nested_analyzer.analyze_function(function_expr);

                // Variables captured by nested functions become captures of this function too
                for (name, captured_var) in nested_analysis.captured_variables {
                    // Increase scope level since we're one level deeper
                    let mut nested_captured = captured_var.clone();
                    nested_captured.scope_level += 1;
                    analysis.add_captured_variable(name, nested_captured);
                }
            }
            TypedExpressionValue::Call(call_expr) => {
                self.analyze_expression(&call_expr.callee, analysis);
                for arg in &call_expr.arguments {
                    self.analyze_expression(arg, analysis);
                }
            }
            TypedExpressionValue::Assignment(assignment_expr) => {
                // For assignments, the identifier is directly available
                self.analyze_identifier(&assignment_expr.identifier, analysis);
                self.analyze_expression(&assignment_expr.value, analysis);
            }
            TypedExpressionValue::Block(block_expr) => {
                // Create new scope for block
                let mut block_env = self.type_env.block();

                for statement in &block_expr.statements {
                    self.analyze_statement(statement, analysis, &mut block_env);
                }

                // Analyze the result expression with block environment
                let analyzer = CaptureAnalyzer {
                    type_env: &block_env,
                    scope_depth: self.scope_depth,
                };
                analyzer.analyze_expression(&block_expr.result, analysis);
            }
            TypedExpressionValue::Conditional(conditional_expr) => {
                self.analyze_expression(&conditional_expr.condition, analysis);
                self.analyze_expression(&conditional_expr.truthy, analysis);
                self.analyze_expression(&conditional_expr.falsy, analysis);
            }
            TypedExpressionValue::Binary(binary_expr) => {
                self.analyze_expression(&binary_expr.left, analysis);
                self.analyze_expression(&binary_expr.right, analysis);
            }
            TypedExpressionValue::Unary(unary_expr) => {
                self.analyze_expression(&unary_expr.operand, analysis);
            }
            TypedExpressionValue::FieldAccess(field_expr) => {
                self.analyze_expression(&field_expr.object, analysis);
            }
            TypedExpressionValue::StructConstructor(struct_expr) => {
                for argument in &struct_expr.arguments {
                    self.analyze_expression(&argument.value, analysis);
                }
            }
            TypedExpressionValue::Group(group_expr) => {
                self.analyze_expression(&group_expr.expression, analysis);
            }
            // Literals don't capture anything
            TypedExpressionValue::Primary(_) => {}
        }
    }

    /// Analyze a statement for captured variables
    fn analyze_statement(
        &self,
        stmt: &TypedStatement,
        analysis: &mut CaptureAnalysis,
        block_env: &mut TypeEnvironment,
    ) {
        match &stmt.value {
            StatementValue::Expression(expr) => {
                // Analyze the expression
                self.analyze_expression(expr, analysis);
            }
            StatementValue::VariableDeclaration(var_decl) => {
                // Analyze the initialization expression
                self.analyze_expression(&var_decl.value, analysis);

                // Add the variable to local scope
                block_env.declare(&var_decl.identifier, &var_decl.value.type_);
                analysis.add_local_variable(
                    var_decl.identifier.name.to_string(),
                    var_decl.value.type_.value.clone(),
                );
            }
            StatementValue::TypeDeclaration(_)
            | StatementValue::ExternDeclaration(_)
            | StatementValue::Import(_) => {
                // These don't capture variables
            }
        }
    }

    /// Analyze an identifier to see if it's captured
    fn analyze_identifier(&self, identifier: &Identifier, analysis: &mut CaptureAnalysis) {
        // Check if it's already known as local or captured
        if analysis.is_variable_local(&identifier.name)
            || analysis.is_variable_captured(&identifier.name)
        {
            return;
        }

        // Try to find the variable in the environment chain
        if let Some(declaration) = self.find_variable_in_scope_chain(identifier) {
            let captured_var = CapturedVariable {
                name: identifier.name.to_string(),
                type_: declaration.type_.value.clone(),
                scope_level: declaration.scope_level,
                capture_mode: CaptureMode::ByValue, // For now, always capture by value
            };

            analysis.add_captured_variable(identifier.name.to_string(), captured_var);
        } else {
            // Variable not found - this should be a type error, but we'll record it
            analysis.add_unresolved_variable(identifier.name.to_string());
        }
    }

    /// Find a variable declaration in the scope chain
    fn find_variable_in_scope_chain(&self, identifier: &Identifier) -> Option<VariableDeclaration> {
        let mut current_env = Some(self.type_env);
        let mut scope_level = 0;

        while let Some(env) = current_env {
            if let Some(type_) = env.declarations.get(&identifier) {
                return Some(VariableDeclaration {
                    type_: type_.clone(),
                    scope_level,
                });
            }

            current_env = env.parent;
            scope_level += 1;
        }

        None
    }
}

/// Information about a variable declaration found in the scope chain
#[derive(Debug, Clone)]
struct VariableDeclaration {
    type_: Type,
    scope_level: usize,
}
