use clap_file::LockedInput;
use miette::SourceCode;

use crate::prelude::*;
use std::{
    io::{BufRead, Read},
    path::{Path, PathBuf},
};

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
    #[command(subcommand)]
    commands: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Run a source file
    Run { file: clap_file::Input },
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

    match cli.commands {
        Commands::Run { file } => run(file.lock()),
    }
}

fn run(mut input: LockedInput) {
    let mut content = String::new();
    input
        .read_to_string(&mut content)
        .expect("Failed to read input");

    let name: String = input
        .path()
        .map(|p: &Path| p.display().to_string())
        .unwrap_or_else(|| "<stdin>".to_string());

    let source = miette::NamedSource::new(name, content);

    let lexer = Lexer::new(&source.inner().as_str());

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
