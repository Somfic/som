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
use transpiler::{bend::BendTranspiler, Transpiler};

pub mod parser;
pub mod scanner;
pub mod transpiler;

fn main() -> Result<()> {
    let code = "struct Result { ok: number, error: string }";
    let file = SimpleFile::new("main", code);

    let tokens = scanner::Scanner::new(code.to_owned()).collect::<Vec<_>>();
    let mut parser = parser::Parser::new(tokens);
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
                            Label::primary(
                                (),
                                diagnostic.range.position
                                    ..diagnostic.range.position + diagnostic.range.length,
                            )
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
