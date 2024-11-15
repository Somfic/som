use std::fmt::format;

use crate::parser::ast::{
    Expression, ExpressionValue, Primitive, Statement, StatementValue, Symbol,
};

pub struct CodeGenerator<'de> {
    indent_level: usize,
    symbol: &'de Symbol<'de>,
}

impl<'de> CodeGenerator<'de> {
    // create a new builder with no indentation
    pub fn new(symbol: &'de Symbol<'de>) -> Self {
        Self {
            indent_level: 0,
            symbol,
        }
    }

    // compile the entire symbol
    pub fn compile(&mut self) -> String {
        match self.symbol {
            Symbol::Statement(statement) => self.compile_statement(statement),
            Symbol::Expression(expression) => self.compile_expression(expression),
        }
    }

    // increase the indentation level
    fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    // decrease the indentation level
    fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    // get the current indentation as a string of spaces
    fn current_indent(&self) -> String {
        "  ".repeat(self.indent_level) // adjust spaces per level as needed
    }

    // compile a block of statements, maintaining indentation
    fn compile_block(&mut self, statements: &[Statement]) -> String {
        let mut code = String::new();
        self.increase_indent();
        for stmt in statements {
            code.push_str(&format!(
                "{}{}\n",
                self.current_indent(),
                self.compile_statement(stmt)
            ));
        }
        self.decrease_indent();
        code
    }

    // compile an individual statement
    fn compile_statement(&mut self, stmt: &Statement) -> String {
        match &stmt.value {
            StatementValue::Assignment { name, value } => {
                format!("var {} = {};", name, self.compile_expression(value))
            }
            StatementValue::Return(expression) => {
                format!("return {};", self.compile_expression(expression))
            }
            StatementValue::Block(block_statements) => {
                let mut block_code = String::from("{\n");
                block_code.push_str(&self.compile_block(block_statements));
                block_code.push_str(&format!("{}}}", self.current_indent()));
                block_code
            }
            StatementValue::Function { header, body } => {
                let mut code = format!("function {}(", header.name);
                for (i, parameter) in header.parameters.iter().enumerate() {
                    if i > 0 {
                        code.push_str(", ");
                    }
                    code.push_str(&parameter.name);
                }

                code.push_str(") {\n");
                code.push_str(&self.compile_expression(body));
                code.push_str(&format!("\n{}}}", self.current_indent()));

                code
            }
            StatementValue::Expression(expression) => {
                format!("{}", self.compile_expression(expression))
            }
            StatementValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let mut code = format!("if {} {{\n", self.compile_expression(condition));
                self.increase_indent();
                code.push_str(&self.compile_statement(truthy));
                self.decrease_indent();
                code.push_str(&format!("\n{}}}", self.current_indent()));
                if let Some(falsy) = falsy {
                    code.push_str(" else {\n");
                    self.increase_indent();
                    code.push_str(&self.compile_statement(falsy));
                    self.decrease_indent();
                    code.push_str(&format!("\n{}}}", self.current_indent()));
                }
                code
            }
            _ => {
                println!("{:?}", stmt);
                todo!("handle more statement types")
            }
        }
    }

    // compile an expression (placeholder for your full expression handling)
    fn compile_expression(&mut self, expr: &Expression) -> String {
        match &expr.value {
            ExpressionValue::Primitive(primitive) => primitive.to_string(),
            ExpressionValue::Binary {
                operator,
                left,
                right,
            } => {
                format!(
                    "({} {} {})",
                    self.compile_expression(left),
                    operator,
                    self.compile_expression(right)
                )
            }
            ExpressionValue::Group(inner) => self.compile_expression(inner).to_string(),
            ExpressionValue::Block {
                statements,
                return_value,
            } => {
                let mut code = String::new();
                code.push_str(&self.compile_block(statements));
                self.increase_indent();

                if let ExpressionValue::Primitive(Primitive::Unit) = return_value.value {
                } else {
                    code.push_str(&format!(
                        "{}return {};",
                        self.current_indent(),
                        self.compile_expression(return_value)
                    ));
                }

                self.decrease_indent();
                code
            }
            ExpressionValue::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                format!(
                    "{} ? {} : {}",
                    self.compile_expression(condition),
                    self.compile_expression(truthy),
                    self.compile_expression(falsy)
                )
            }
            _ => {
                println!("{:?}", expr);
                todo!("handle more expression types")
            }
        }
    }
}
