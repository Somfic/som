use super::Transpiler;
use crate::parser::ast::Symbol;
use anyhow::Result;
use deno_core::{JsRuntime, RuntimeOptions};

pub mod expressions;
pub mod statements;

pub struct JavaScriptTranspiler {
    runtime: JsRuntime,
    code: String,
}

impl Transpiler for JavaScriptTranspiler {
    fn transpile(symbol: &Symbol) -> String {
        match symbol {
            Symbol::Statement(statement) => statements::transpile(statement),
            Symbol::Expression(expression) => expressions::transpile(expression),
            Symbol::Type(_) => "".to_string(),
        }
    }
}
