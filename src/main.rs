use anyhow::Result;
use codespan_reporting::term::{
    self,
    termcolor::{ColorChoice, StandardStream},
};
use core::result::Result::Ok;
use diagnostic::Diagnostic;
use files::Files;
use std::{collections::HashSet, env::args, fs::read};
use transpiler::{bend::BendTranspiler, Transpiler};

pub mod diagnostic;
pub mod files;
pub mod parser;
pub mod scanner;
pub mod transpiler;

fn main() -> Result<()> {
    let args: Vec<String> = args().skip(1).collect();

    let file = args.first().unwrap();
    let source = &String::from_utf8(read(file).unwrap()).unwrap();

    let mut files = Files::default();
    files.insert(file, source);

    let scanner = scanner::Scanner::new(&files);
    let (tokens, scanner_diagnostics) = scanner.parse();

    print_diagnostics(scanner_diagnostics, &files);

    let mut parser = parser::Parser::new(&tokens);
    let (ast, parser_diagnostics) = parser.parse();

    print_diagnostics(parser_diagnostics, &files);

    Ok(())
}

fn print_diagnostics(diagnostics: HashSet<Diagnostic>, files: &Files) {
    for diagnostic in diagnostics.iter() {
        println!("{:?}", diagnostic);
    }

    let diagnostics: Vec<codespan_reporting::diagnostic::Diagnostic<&str>> =
        diagnostics.iter().map(|d| d.clone().into()).collect();

    let writer = StandardStream::stderr(ColorChoice::Auto);
    let config = codespan_reporting::term::Config::default();

    for diagnostic in diagnostics {
        term::emit(&mut writer.lock(), &config, files, &diagnostic).unwrap();
    }
}
