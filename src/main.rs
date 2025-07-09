use crate::prelude::*;
use parser::Parser;

mod compiler;
mod expressions;
mod lexer;
mod parser;
mod prelude;
mod runner;
mod statements;
mod type_checker;
mod types;

fn main() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .with_cause_chain()
                .context_lines(2)
                .build(),
        )
    }))
    .unwrap();

    let source = "let this_is_a_variable = 1; let this_is_a_variabl = 12; thisisavariable";

    let lexer = Lexer::new(source);

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(errors) => {
            for error in errors {
                eprintln!("{:?}", miette::miette!(error).with_source_code(source));
            }
            std::process::exit(1);
        }
    };

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(errors) => {
            for error in errors {
                eprintln!("{:?}", miette::miette!(error).with_source_code(source));
            }
            std::process::exit(1);
        }
    };

    let mut compiler = Compiler::new();
    let compiled = compiler.compile(&type_checked);

    let runner = Runner::new();
    let ran = runner.run(compiled).unwrap();

    println!("Result: {ran}");
}
