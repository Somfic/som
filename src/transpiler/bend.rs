use crate::parser::{BinaryOperation, Expression, Statement, Symbol};

use super::Transpiler;

pub struct BendTranspiler {}

impl Transpiler for BendTranspiler {
    fn transpile(symbol: &Symbol) -> String {
        let mut output = String::new();

        match symbol {
            Symbol::Expression(expression) => {
                output.push_str(&transpile_expression(expression));
            }
            Symbol::Statement(statement) => {
                output.push_str(&transpile_statement(statement));
            }
            Symbol::Unknown(lexeme) => {
                output.push_str(&format!("Unknown lexeme: {:?}", lexeme));
            }
        }

        output
    }
}

#[allow(clippy::only_used_in_recursion)]
fn transpile_expression(expression: &Expression) -> String {
    match expression {
        Expression::Number(number) => number.to_string(),
        Expression::String(string) => format!("\"{}\"", string),
        Expression::Identifier(symbol) => symbol.clone(),
        Expression::Binary(left, operation, right) => {
            let left = transpile_expression(left);
            let operation = match operation {
                BinaryOperation::Plus => "+",
                BinaryOperation::Minus => "-",
                BinaryOperation::Times => "*",
                BinaryOperation::Divide => "/",
            };
            let right = transpile_expression(right);
            format!("{} {} {}", left, operation, right)
        }
        Expression::Grouping(expression) => {
            format!("({})", transpile_expression(expression))
        }
    }
}

fn transpile_statement(statement: &Statement) -> String {
    match statement {
        Statement::Block(statements) => {
            let mut output = String::new();
            output.push_str("{\n");
            for statement in statements {
                output.push_str(&format!("  {}", transpile_statement(statement)));
            }
            output.push_str("}\n");
            output
        }
        Statement::Expression(expression) => {
            let expression = transpile_expression(expression);
            format!("{};\n", expression)
        }
    }
}
