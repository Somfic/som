use crate::{compiler::Compiler, parser::Parser, runner::Runner, typer::Typer};

mod binary;
mod group;
mod unary;

pub fn run_and_assert(source_code: &str, expected: i64) {
    let expression = Parser::new(source_code).parse().unwrap();
    let expression = Typer::new(expression).type_check().unwrap();
    let compiled = Compiler::new(expression).compile().unwrap();
    let result = Runner::new(compiled).run().unwrap();

    if result != expected {
        println!("got {}, expected {}\n{}", result, expected, source_code);
    }

    assert_eq!(result, expected);
}
