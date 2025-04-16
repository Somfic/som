mod prelude;

use std::path::PathBuf;

use ast::TypedModule;
use clap::arg;
use clap::command;
use clap::Args;
use clap::Parser;
use highlighter::SomHighlighter;
pub use prelude::*;

mod ast;
mod compiler;
mod highlighter;
mod parser;
mod runner;
#[cfg(test)]
mod tests;
mod tokenizer;
mod typer;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct SomArgs {
    #[arg(short, long)]
    pub source_path: PathBuf,
}

fn main() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(2)
                .with_syntax_highlighting(SomHighlighter {})
                .build(),
        )
    }))
    .unwrap();

    let args = SomArgs::parse();

    let source = std::fs::read_to_string(args.source_path).unwrap_or_else(|_| {
        eprintln!("Failed to read source file.");
        std::process::exit(1);
    });

    let result = runner::run(source);
    println!("{}", result)
}
