mod prelude;
use ast::{Type, TypedExpression, TypedStatement};
use cranelift::codegen::CompiledCode;
use miette::miette;
pub use prelude::*;

mod ast;
mod compiler;
mod parser;
mod runner;
#[cfg(test)]
mod tests;
mod tokenizer;
mod typer;

const INPUT: &str = "{ let b = { 1 + 1; 1 }; b + 1 }";

fn main() {
    let result = tests::run(source_code);
    println!("Result: {}", result);
}

fn parse(source_code: &str) -> ParserResult<Vec<TypedStatement<'_>>> {
    let statements = parser::Parser::new(source_code).parse()?;
    let statements = typer::Typer::new().type_check(statements)?;
}

fn compile(statements: Vec<TypedStatement<'_>>) -> CompilerResult<CompiledCode> {
    compiler::Compiler::new().compile(statements)
}
