use miette::NamedSource;

use std::{io::Read, path::PathBuf};

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

    /// Watch for file changes and recompile automatically
    #[arg(long, short)]
    watch: bool,
}

fn main() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .with_cause_chain()
                .context_lines(1)
                .with_syntax_highlighting(SomHighlighter {})
                .build(),
        )
    }))
    .unwrap();

    let cli = <Cli as clap::Parser>::parse();
    let mut source = cli.source;

    if source.is_dir() {
        source = source.join("main.som");
    }

    // check if file exists
    if !source.exists() {
        tui::print_error(format!("source file `{}` does not exist", source.display()));
        std::process::exit(1);
    }

    if cli.watch {
        // Run in watch mode
        cli::run_watch_mode(source);
    } else {
        // Run once
        run_once(source);
    }
}

fn run_once(source: PathBuf) {
    // read the source file
    let mut content = String::new();
    if let Err(e) =
        std::fs::File::open(&source).and_then(|mut file| file.read_to_string(&mut content))
    {
        tui::print_error(format!(
            "error reading source file '{}': {}",
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
    
    // Print the execution result if successful
    if let Some(value) = result {
        println!("  ⚡ Result: {}", value);
    }
}
