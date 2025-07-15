use clap_file::LockedInput;
use miette::{NamedSource, SourceCode};

use crate::{
    prelude::*,
    tui::{start_animated_display, Process, ProcessState},
};
use std::time::SystemTime;
use std::{
    io::{BufRead, Read},
    path::{Path, PathBuf},
};

mod compiler;
mod compiler_example;
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
        file: clap_file::Input,
    },
    ProcessTree,
    /// Display a realistic compiler simulation
    CompilerDemo,
    /// Display a failed compilation example
    CompilerFailed,
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
        Commands::ProcessTree => {
            let now = SystemTime::now();
            let process = Process {
                name: "root".to_string(),
                state: ProcessState::Compiling,
                started_at: now - std::time::Duration::from_secs(120),
                completed_at: None,
                children: vec![
                    Process {
                        name: "child1".to_string(),
                        state: ProcessState::Compiling,
                        started_at: now - std::time::Duration::from_secs(60),
                        completed_at: None,
                        children: vec![
                            Process {
                                name: "grandchild1".to_string(),
                                state: ProcessState::Waiting,
                                started_at: now - std::time::Duration::from_secs(10),
                                completed_at: None,
                                children: vec![],
                            },
                            Process {
                                name: "grandchild2".to_string(),
                                state: ProcessState::Error,
                                started_at: now - std::time::Duration::from_secs(30),
                                completed_at: Some(now - std::time::Duration::from_secs(25)),
                                children: vec![Process {
                                    name: "great_grandchild".to_string(),
                                    state: ProcessState::Waiting,
                                    started_at: now - std::time::Duration::from_secs(5),
                                    completed_at: None,
                                    children: vec![],
                                }],
                            },
                        ],
                    },
                    Process {
                        name: "child2".to_string(),
                        state: ProcessState::Completed,
                        started_at: now - std::time::Duration::from_secs(30),
                        completed_at: Some(now - std::time::Duration::from_secs(20)),
                        children: vec![],
                    },
                ],
            };
            start_animated_display(process);
        }
        Commands::CompilerDemo => {
            println!("🔨 Starting realistic compiler simulation...");
            println!("This shows how library dependencies compile in a realistic order.\n");
            compiler_example::run_compiler_simulation();
        }
        Commands::CompilerFailed => {
            println!("❌ Showing failed compilation example...");
            println!("This demonstrates how errors propagate through dependency trees.\n");
            compiler_example::run_failed_compilation_example();
        }
    }
}
