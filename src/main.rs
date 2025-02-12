mod prelude;
pub use prelude::*;

mod compiler;
mod runner;
#[cfg(test)]
mod tests;
mod tokenizer;

const INPUT: &str = "
fn main() {
    let a = 1;
}
";

fn main() {
    if let Err(e) = run() {
        for error in e {
            eprintln!("{}", error);
        }
    }
}

fn run() -> Result<()> {
    let compiled = compiler::compile(INPUT)?;
    let result = runner::run(compiled)?;

    println!("{:?}", result);
    Ok(())
}
