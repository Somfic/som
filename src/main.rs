mod prelude;
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

const INPUT: &str = "1+1";

fn main() {
    if let Err(e) = run(INPUT) {
        for error in e {
            eprintln!("{:?}", miette!(error).with_source_code(INPUT));
        }
    }
}

fn run(source_code: &str) -> Result<()> {
    let expression = parser::Parser::new(source_code).parse()?;
    let typed = typer::Typer::new(expression).type_check()?;
    let compiled = compiler::Compiler::new(typed).compile()?;
    let result = runner::Runner::new(compiled).run()?;

    println!("{:?}", result);
    Ok(())
}
