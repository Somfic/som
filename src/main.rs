use clap_file::LockedInput;
use miette::{NamedSource, SourceCode};

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

#[cfg(test)]
mod tests;

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
        Commands::Run { mut file } => {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .expect("Failed to read input");

            let name: String = file
                .path()
                .map(|p: &Path| p.display().to_string())
                .unwrap_or_else(|| "<stdin>".to_string());

            let source = miette::NamedSource::new(name, content);

            let result = run(source);

            println!("Result: {:?}", result);
        }
    }
}
