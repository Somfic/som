pub mod captures;
pub mod metadata;
pub mod tail_calls;

use crate::prelude::*;
pub use metadata::LoweringMetadata;

/// The lowering module analyzes and transforms typed AST.
/// This pass runs between type checking and compilation.
pub struct Lowering {
    pub metadata: LoweringMetadata,
}

impl Lowering {
    pub fn new() -> Self {
        Self {
            metadata: LoweringMetadata::new(),
        }
    }

    /// Apply all lowering passes to a typed statement
    /// This analyzes the AST and collects optimization metadata
    pub fn lower(&mut self, statement: TypedStatement) -> TypedStatement {
        // Walk the statement tree and mark tail-recursive functions
        self.analyze_statement(&statement);

        // For now, we don't transform the AST itself
        // The metadata is used by the compiler to generate optimized code
        statement
    }

    fn analyze_statement(&mut self, statement: &TypedStatement) {
        match &statement.value {
            StatementValue::VariableDeclaration(decl) => {
                // Check if this is a function declaration
                if let TypedExpressionValue::Function(_) = &decl.value.value {
                    // Check if it's tail-recursive
                    if tail_calls::is_tail_recursive(Some(&decl.identifier), &decl.value) {
                        self.metadata.mark_tail_recursive(decl.identifier.name.to_string());
                    }
                }
            }
            StatementValue::Expression(expr) => {
                self.analyze_expression(expr);
            }
            _ => {}
        }
    }

    fn analyze_expression(&mut self, expression: &TypedExpression) {
        match &expression.value {
            TypedExpressionValue::Block(block) => {
                for stmt in &block.statements {
                    self.analyze_statement(stmt);
                }
                self.analyze_expression(&block.result);
            }
            TypedExpressionValue::Function(func) => {
                self.analyze_expression(&func.body);
            }
            TypedExpressionValue::Conditional(cond) => {
                self.analyze_expression(&cond.condition);
                self.analyze_expression(&cond.truthy);
                self.analyze_expression(&cond.falsy);
            }
            TypedExpressionValue::Call(call) => {
                self.analyze_expression(&call.callee);
                for arg in &call.arguments {
                    self.analyze_expression(arg);
                }
            }
            TypedExpressionValue::Binary(bin) => {
                self.analyze_expression(&bin.left);
                self.analyze_expression(&bin.right);
            }
            TypedExpressionValue::Unary(un) => {
                self.analyze_expression(&un.operand);
            }
            TypedExpressionValue::WhileLoop(while_loop) => {
                self.analyze_expression(&while_loop.condition);
                self.analyze_expression(&while_loop.body);
            }
            TypedExpressionValue::Group(group) => {
                self.analyze_expression(&group.expression);
            }
            TypedExpressionValue::FieldAccess(field) => {
                self.analyze_expression(&field.object);
            }
            TypedExpressionValue::StructConstructor(strukt) => {
                for arg in &strukt.arguments {
                    self.analyze_expression(&arg.value);
                }
            }
            TypedExpressionValue::Assignment(assign) => {
                self.analyze_expression(&assign.value);
            }
            _ => {}
        }
    }
}
