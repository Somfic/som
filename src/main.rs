use clap_file::LockedInput;
use miette::{NamedSource, SourceCode};
use owo_colors::OwoColorize;

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
    source: PathBuf,
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
    let mut source = cli.source;

    if source.is_dir() {
        source = source.join("main.som");
    }

    // check if file exists
    if !source.exists() {
        tui::print_error(format!("Source file `{}` does not exist", source.display()));
        std::process::exit(1);
    }

    // read the source file
    let mut content = String::new();
    if let Err(e) =
        std::fs::File::open(&source).and_then(|mut file| file.read_to_string(&mut content))
    {
        tui::print_error(format!(
            "Error reading source file '{}': {}",
            source.display(),
            e
        ));
        std::process::exit(1);
    }

    let name: String = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("main")
        .to_string();

    let result = cli::run_with_process_tree(NamedSource::new(name, content));

    println!("Result: {:?}", result);
}
