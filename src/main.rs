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
use transpiler::{bend::BendTranspiler, Transpiler};

pub mod diagnostic;
pub mod files;
pub mod parser;
pub mod scanner;
pub mod transpiler;

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

    let mut parser = parser::Parser::new(&lexemes);
    let parsed = parser.parse();

    match &parsed {
        Ok(_) => {}
        Err(diagnostics) => {
            // Print the diagnostics
            for diagnostic in diagnostics.iter() {
                println!("{:?}", diagnostic);
            }

            let diagnostics: Vec<Diagnostic<&str>> =
                diagnostics.iter().map(|d| d.clone().into()).collect();

            let writer = StandardStream::stderr(ColorChoice::Auto);
            let config = codespan_reporting::term::Config::default();

            for diagnostic in diagnostics {
                term::emit(&mut writer.lock(), &config, &files, &diagnostic)?;
            }
        }
    }

    let transpiled = BendTranspiler::transpile(&parsed.unwrap());

    println!("{}", transpiled);

    Ok(())
}
