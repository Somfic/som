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
    let scanner_pass = scanner.parse();

    // scanner_pass.print_diagnostics(&files);

    let mut parser = parser::Parser::new(&scanner_pass.result);
    let parser_pass = parser.parse().unwrap();

    parser.print_diagnostics(&files);

    let transpiler = BendTranspiler::transpile(&parser_pass);

    println!("{}", transpiler);

    Ok(())
}
