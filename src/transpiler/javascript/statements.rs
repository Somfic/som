use std::collections::HashSet;

use crate::parser::ast::{
    EnumMember, Expression, FieldSignature, Function, FunctionSignature, Statement, Type,
};

pub fn transpile(statement: &Statement) -> String {
    match statement {
        Statement::Block(block) => transpile_block(block),
        Statement::Declaration(name, typest, expression) => {
            transpile_declaration(name, typest, expression)
        }
        Statement::Expression(expression) => transpile_expression(expression),
        Statement::Struct(name, fields) => transpile_struct(name, fields),
        Statement::Enum(name, members) => transpile_enum(name, members),
        Statement::Function(function) => transpile_function(function),
        Statement::Return(returning) => transpile_return(returning),
        Statement::Trait(name, functions, fields) => transpile_trait(name, functions, fields),
        Statement::Implementation(name, typest, functions) => {
            transpile_implementation(name, typest, functions)
        }
    }
}

fn transpile_block(block: &Vec<Statement>) -> String {
    block
        .iter()
        .map(|statement| transpile(statement))
        .collect::<Vec<String>>()
        .join("\n")
}

fn transpile_declaration(name: &String, _typest: &Option<Type>, expression: &Expression) -> String {
    format!(
        "var {} = {};",
        name,
        super::expressions::transpile(expression)
    )
}

fn transpile_expression(expression: &Expression) -> String {
    let expression = super::expressions::transpile(expression);
    format!("{};", expression)
}

fn transpile_struct(name: &String, fields: &HashSet<FieldSignature>) -> String {
    "".to_string()
}

fn transpile_enum(name: &String, members: &HashSet<EnumMember>) -> String {
    "".to_string()
}

fn transpile_function(function: &Function) -> String {
    format!(
        "function {}({}) {{ {} }}",
        function.signature.name,
        function
            .signature
            .parameters
            .keys()
            .cloned()
            .collect::<Vec<String>>()
            .join(", "),
        transpile(&function.body)
    )
}

fn transpile_return(returning: &Expression) -> String {
    format!("return {};", super::expressions::transpile(returning))
}

fn transpile_trait(
    _name: &str,
    _functions: &HashSet<FunctionSignature>,
    _fields: &HashSet<FieldSignature>,
) -> String {
    "".to_string()
}

fn transpile_implementation(_name: &str, typest: &str, functions: &HashSet<Function>) -> String {
    functions
        .iter()
        .map(|function| {
            transpile_function(&Function {
                signature: {
                    let mut signature = function.signature.clone();
                    signature.name = format!("__{}_for_{}", signature.name, typest);
                    signature
                },
                body: function.body.clone(),
            })
        })
        .collect::<Vec<String>>()
        .join("\n")
}
