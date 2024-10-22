use std::collections::HashMap;

use crate::parser::ast::{BinaryOperation, Expression, UnaryOperation};

pub fn transpile(expression: &Expression) -> String {
    match expression {
        Expression::Number(number) => transpile_number(number),
        Expression::String(string) => transpile_string(string),
        Expression::Identifier(identifier) => transpile_identifier(identifier),
        Expression::Boolean(boolean) => transpile_boolean(boolean),
        Expression::Unary(operation, expression) => transpile_unary(operation, expression),
        Expression::Binary(left, operation, right) => transpile_binary(left, operation, right),
        Expression::Grouping(expression) => transpile_grouping(expression),
        Expression::Assignment(name, expression) => transpile_assignment(name, expression),
        Expression::StructInitializer(name, fields) => transpile_struct_initializer(name, fields),
        Expression::FunctionCall(name, parameters) => transpile_function_call(name, parameters),
    }
}

fn transpile_number(number: &f64) -> String {
    number.to_string()
}

fn transpile_string(string: &str) -> String {
    format!("\"{}\"", string)
}

fn transpile_identifier(identifier: &str) -> String {
    identifier.to_string()
}

fn transpile_boolean(boolean: &bool) -> String {
    boolean.to_string()
}

fn transpile_unary(operation: &UnaryOperation, expression: &Expression) -> String {
    match operation {
        UnaryOperation::Negate => format!("!{}", transpile(expression)),
        UnaryOperation::Inverse => format!("-{}", transpile(expression)),
    }
}

fn transpile_binary(left: &Expression, operation: &BinaryOperation, right: &Expression) -> String {
    match operation {
        BinaryOperation::Plus => format!("{} + {}", transpile(left), transpile(right)),
        BinaryOperation::Minus => format!("{} - {}", transpile(left), transpile(right)),
        BinaryOperation::Times => format!("{} * {}", transpile(left), transpile(right)),
        BinaryOperation::Divide => format!("{} / {}", transpile(left), transpile(right)),
    }
}

fn transpile_grouping(expression: &Expression) -> String {
    format!("({})", transpile(expression))
}

fn transpile_assignment(name: &Expression, expression: &Expression) -> String {
    format!("{} = {}", transpile(name), transpile(expression))
}

fn transpile_struct_initializer(_name: &str, fields: &HashMap<String, Expression>) -> String {
    let fields = fields
        .iter()
        .map(|(key, value)| format!("{}: {}", key, transpile(value)))
        .collect::<Vec<String>>()
        .join(", ");
    format!("{{ {} }}", fields)
}

fn transpile_function_call(name: &str, parameters: &[Expression]) -> String {
    let parameters = parameters
        .iter()
        .map(transpile)
        .collect::<Vec<String>>()
        .join(", ");

    format!("{}({})", name, parameters)
}
