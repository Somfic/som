use crate::parser::ast::Symbol;

pub mod javascript;

pub trait Transpiler {
    fn transpile(symbol: &Symbol) -> String;
}
