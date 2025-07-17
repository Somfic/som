use clap_file::LockedInput;
use miette::{NamedSource, SourceCode};

use std::time::SystemTime;
use std::{
    io::{BufRead, Read},
    path::{Path, PathBuf},
};

use crate::highlighter::SomHighlighter;

mod cli;
mod compiler;
mod errors;
mod expressions;
mod highlighter;
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
    #[arg(default_value = "main.som")]
    source: clap_file::Input,
}

fn main() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .with_cause_chain()
                .context_lines(1)
                // .with_syntax_highlighting(SomHighlighter {})
                .build(),
        )
    }))
    .unwrap();

    let mut cli = <Cli as clap::Parser>::parse();

    let mut content = String::new();
    cli.source
        .read_to_string(&mut content)
        .expect("Failed to read input");

    let name: String = cli
        .source
        .path()
        .map(|p: &Path| p.display().to_string())
        .unwrap_or_else(|| "<stdin>".to_string());

    let source = miette::NamedSource::new(name, content);

    let result = cli::run_with_process_tree(source);

    println!("Result: {:?}", result);
}
