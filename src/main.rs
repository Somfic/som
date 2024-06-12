use anyhow::Result;
use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFile,
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};
use core::result::Result::Ok;
use scanner::lexeme::{Lexeme, Range};
use transpiler::{bend::BendTranspiler, Transpiler};

pub mod parser;
pub mod scanner;
pub mod transpiler;

fn lexeme_range_to_source_range(lexemes: &[Lexeme], diagnostic: &parser::Diagnostic) -> Range {
    let start = if diagnostic.range.position >= lexemes.len() {
        let last_lexeme = lexemes[lexemes.len() - 1].range();
        last_lexeme.position + 1
    } else {
        let start_lexeme = lexemes[diagnostic.range.position].range();
        start_lexeme.position
    };

    let end = if diagnostic.range.position + diagnostic.range.length >= lexemes.len() {
        let last_lexeme = lexemes[lexemes.len() - 1].range();
        last_lexeme.position + last_lexeme.length
    } else {
        let end_lexeme = lexemes[diagnostic.range.position + diagnostic.range.length].range();
        end_lexeme.position + end_lexeme.length
    };

    Range {
        position: start,
        length: end - start,
    }
}

fn main() -> Result<()> {
    let code = "
        1 = (1));
    ";
    let file: SimpleFile<&str, &str> = SimpleFile::new("main", code);

    let lexemes = scanner::Scanner::new(code.to_owned()).collect::<Vec<_>>();
    let mut parser = parser::Parser::new(&lexemes);
    let parsed = parser.parse();

    match &parsed {
        Ok(_) => {}
        Err(diagnostics) => {
            let diagnostic: Diagnostic<()> = Diagnostic::error()
                .with_message("Syntax error")
                .with_labels(
                    diagnostics
                        .iter()
                        .map(|diagnostic| {
                            let range = lexeme_range_to_source_range(&lexemes, &diagnostic);

                            Label::primary((), range.position..range.position + range.length)
                                .with_message(diagnostic.message.to_string())
                        })
                        .collect(),
                );

            let writer = StandardStream::stderr(ColorChoice::Auto);
            let config = codespan_reporting::term::Config::default();
            term::emit(&mut writer.lock(), &config, &file, &diagnostic)?;
        }
    }

    let transpiled = BendTranspiler::transpile(&parsed.unwrap());

    println!("{}", transpiled);

    Ok(())
}
