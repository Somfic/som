use clap_file::LockedInput;
use miette::{NamedSource, SourceCode};

use crate::tui::{start_animated_display, Process, ProcessState};
use std::time::SystemTime;
use std::{
    io::{BufRead, Read},
    path::{Path, PathBuf},
};

mod cli;
mod compiler;
mod errors;
mod expressions;
mod lexer;
mod parser;
mod prelude;
mod runner;
mod statements;
mod tui;
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
    Run {
        source: clap_file::Input,
        /// Disable process tree visualization and show raw output
        #[arg(long)]
        raw: bool,
    },
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
        Commands::Run {
            source: mut file,
            raw,
        } => {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .expect("Failed to read input");

            let name: String = file
                .path()
                .map(|p: &Path| p.display().to_string())
                .unwrap_or_else(|| "<stdin>".to_string());

            let source = miette::NamedSource::new(name, content);

            let result = if raw {
                // Use raw output without process tree
                cli::run(source)
            } else {
                // Use process tree visualization by default
                cli::run_with_process_tree(source)
            };

            println!("Result: {:?}", result);
        }
    }
}
