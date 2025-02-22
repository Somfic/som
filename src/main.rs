mod prelude;
use ast::{Type, TypedExpression};
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

const INPUT: &str = "10 * 1 - 1";

fn main() {
    println!("{}\n", INPUT);

    let expression = parse(INPUT)
        .map_err(|errors| {
            for error in errors {
                eprintln!("{:?}", miette!(error).with_source_code(INPUT));
            }
        })
        .expect("failed to parse expression");

    let compiled = compile(expression)
        .map_err(|error| {
            for error in error {
                eprintln!("{:?}", error);
            }
        })
        .expect("failed to compile expression");

    let result = runner::Runner::new(compiled)
        .run()
        .expect("failed to run expression");

    println!("{}", result);
}

fn parse(source_code: &str) -> ParserResult<TypedExpression<'_>> {
    let expression = parser::Parser::new(source_code).parse()?;
    typer::Typer::new(expression).type_check()
}

fn compile(expression: TypedExpression<'_>) -> CompilerResult<CompiledCode> {
    compiler::Compiler::new(expression).compile()
}
