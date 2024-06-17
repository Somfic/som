use anyhow::Result;
use core::result::Result::Ok;
use files::Files;
use parser::Grammar;

pub mod diagnostic;
pub mod files;
pub mod parser;
pub mod scanner;

fn main() -> Result<()> {
    let mut files = Files::default();
    files.insert(
        "main",
        "
        12 + 12;
    ",
    );

    let scanner = scanner::Scanner::new(&files);
    let tokens = scanner.parse();

    let tokens = match &tokens {
        Ok(tokens) => tokens,
        Err(diagnostics) => {
            diagnostics
                .iter()
                .for_each(|diagnostic| diagnostic.print(&files));
            panic!("Failed to scan");
        }
    };

    let parser = parser::EarleyParser::new(Grammar::default());
    let ast = parser.parse(tokens);

    let _ast = match &ast {
        Ok(ast) => ast,
        Err(diagnostics) => {
            diagnostics
                .iter()
                .for_each(|diagnostic| diagnostic.print(&files));
            panic!("Failed to parse");
        }
    };

    Ok(())
}
