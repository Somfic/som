use anyhow::Result;
use codespan_reporting::{
    diagnostic::Diagnostic,
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};
use core::result::Result::Ok;
use files::Files;
use parser::Grammar;
use scanner::lexeme;

pub mod diagnostic;
pub mod files;
pub mod parser;
pub mod scanner;

fn main() -> Result<()> {
    let mut files = Files::default();
    files.insert(
        "main",
        "
        12 + 12
    ",
    );

    let scanner = scanner::Scanner::new(&files);
    let lexemes = scanner.parse();

    let lexemes = match &lexemes {
        Ok(lexemes) => lexemes,
        Err(diagnostics) => {
            diagnostics
                .iter()
                .for_each(|diagnostic| diagnostic.print(&files));
            panic!("Failed to scan");
        }
    };

    Ok(())
}
