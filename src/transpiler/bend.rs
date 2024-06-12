use crate::parser::ast::{BinaryOperation, Expression, Statement, Symbol, Type, UnaryOperation};

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
            Symbol::Type(_) => {}
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
        Expression::Assignment(identifier, expression) => {
            let identifier = transpile_expression(identifier);
            let expression = transpile_expression(expression);
            format!("{} = {}", identifier, expression)
        }
        Expression::Unary(operation, expression) => {
            let operation = match operation {
                UnaryOperation::Negate => "-",
                UnaryOperation::Inverse => "!",
            };
            let expression = transpile_expression(expression);
            format!("{}{}", operation, expression)
        }
        Expression::StructInitializer(name, members) => {
            let mut output = String::new();
            output.push_str(&format!("{} {{\n", name));
            for (member, expression) in members {
                output.push_str(&format!(
                    "{}: {},\n",
                    member,
                    transpile_expression(expression)
                ));
            }
            output.push('}');
            output
        }
    }
}

fn transpile_statement(statement: &Statement) -> String {
    match statement {
        Statement::Block(statements) => {
            let mut output = String::new();
            output.push_str("{\n");
            for statement in statements {
                output.push_str(&transpile_statement(statement));
            }
            output.push_str("}\n");
            output
        }
        Statement::Expression(expression) => {
            let expression = transpile_expression(expression);
            format!("{};\n", expression)
        }
        Statement::Declaration(identifier, typing, expression) => {
            let expression = transpile_expression(expression);
            let typing = match typing {
                Some(typing) => format!(": {}", transpile_type(typing)),
                None => String::new(),
            };
            format!("let {}{} = {};\n", identifier, typing, expression)
        }
        Statement::Struct(name, members) => {
            let mut output = String::new();
            output.push_str(&format!("struct {} {{\n", name));
            for (member, typing) in members {
                output.push_str(&format!("{}: {};\n", member, transpile_type(typing)));
            }
            output.push_str("}\n");
            output
        }
        Statement::Enum(name, members) => {
            let mut output = String::new();
            output.push_str(&format!("enum {} {{\n", name));
            for member in members {
                output.push_str(&format!("{},\n", member));
            }
            output.push_str("}\n");
            output
        }
    }
}

fn transpile_type(typing: &Type) -> String {
    match typing {
        Type::Symbol(symbol) => symbol.clone(),
        Type::Array(typing) => format!("{}[]", transpile_type(typing)),
        Type::Tuple(members) => {
            let mut output = String::new();
            output.push('(');
            for (member, typing) in members {
                output.push_str(&format!("{}: {}, ", member, transpile_type(typing)));
            }
            output.push(')');
            output
        }
    }
}
