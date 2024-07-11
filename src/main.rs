use anyhow::{Ok, Result};
use codespan_reporting::term::{
    self,
    termcolor::{ColorChoice, StandardStream},
};

use diagnostic::{Diagnostic, Snippet};
use files::Files;
use std::{collections::HashSet, env::args, fs::read};
use transpiler::{javascript::JavaScriptTranspiler, Transpiler};

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
    let scanner_pass = scanner.parse();

    // let mut debug = Diagnostic::warning("", "");

    // scanner_pass.result.iter().for_each(|token| {
    //     debug = debug.clone().with_snippet(Snippet::secondary_from_token(
    //         token,
    //         token.token_type.to_string(),
    //     ));
    // });

    // let writer = StandardStream::stderr(ColorChoice::Always);
    // let config = term::Config::default();

    // term::emit(&mut writer.lock(), &config, &files, &debug.into()).unwrap();

    println!(
        "{:?}",
        scanner_pass
            .result
            .iter()
            .map(|t| &t.token_type)
            .collect::<Vec<_>>()
    );

    // scanner_pass.print_diagnostics(&files);

    let mut parser = parser::Parser::new(&scanner_pass.result);
    let parser_pass = parser.parse().unwrap();

    parser.print_diagnostics(&files);

    let mut transpiled = JavaScriptTranspiler::transpile(&parser_pass);

    println!("{}", transpiled);

    Ok(())
}
