use crate::parser::Symbol;

pub mod bend;

pub trait Transpiler {
    fn transpile(symbol: &Symbol) -> String;
}
