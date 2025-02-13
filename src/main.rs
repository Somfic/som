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

const INPUT: &str = "1+2";

fn main() {
    if let Err(e) = run(INPUT) {
        for error in e {
            eprintln!("{:?}", miette!(error).with_source_code(INPUT));
        }
    }
}

fn run(source_code: &str) -> Result<()> {
    let compiled = compiler::compile(source_code)?;
    let result = runner::run(compiled)?;

    println!("{:?}", result);
    Ok(())
}
