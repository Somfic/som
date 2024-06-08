use anyhow::Result;
use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFile,
    term::{self, termcolor::StandardStream},
};
use core::result::Result::Ok;

pub mod parser;
pub mod scanner;

fn main() -> Result<()> {
    let code = "'1' * 1 + 1 z 1 - 1;";
    let file = SimpleFile::new("main", code);

    let tokens = scanner::Scanner::new(code.to_owned()).collect::<Vec<_>>();
    let mut parser = parser::Parser::new(tokens);
    let parsed = parser.parse();

    match parsed {
        Ok(_) => {}
        Err(diagnostic) => {
            let writer =
                StandardStream::stderr(codespan_reporting::term::termcolor::ColorChoice::Always);
            let config = codespan_reporting::term::Config::default();
            term::emit(
                &mut writer.lock(),
                &config,
                &file,
                &Diagnostic::error()
                    .with_message(diagnostic.message)
                    .with_labels(vec![Label::primary(
                        (),
                        diagnostic.range.position
                            ..diagnostic.range.position + diagnostic.range.length,
                    )]),
            )?;
        }
    }

    Ok(())
}
