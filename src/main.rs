use crate::prelude::*;
use std::path::PathBuf;

mod compiler;
mod expressions;
mod lexer;
mod parser;
mod prelude;
mod runner;
mod statements;
mod type_checker;
mod types;

#[derive(clap::Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    file: PathBuf,
}

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

    let cli = <Cli as clap::Parser>::parse();

    let source = match std::fs::read_to_string(&cli.file) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("Error reading file {}: {}", cli.file.display(), err);
            std::process::exit(1);
        }
    };

    let lexer = Lexer::new(&source);

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
    };

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
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
